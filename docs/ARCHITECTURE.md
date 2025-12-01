# Proxifier 分流代理系统架构设计

## 1. 高层架构概览
```
+---------------------------+            +-----------------------------+
|        用户交互层          |            |        配置与日志层          |
|  - CLI (clap)             |            |  - YAML 配置 (serde)        |
|  - GUI (druid/gtk-rs)     |            |  - 结构化日志 (tracing)      |
+-------------+-------------+            +-------------+---------------+
              |                                        |
              v                                        v
+---------------------------+            +-----------------------------+
|        核心业务层          |<-----------|       规则/策略层            |
|  - 连接调度器             |            |  - 规则解析/匹配 (IP/域名/进程)|
|  - 流量处理管线           |            |  - 决策函数式组合             |
|  - 上游代理选择器         |            |  - 热更新/验证               |
+-------------+-------------+            +-------------+---------------+
              |                                        ^
              v                                        |
+---------------------------+            +-----------------------------+
|        平台适配层          |            |        外部依赖              |
|  - Windows WFP Callout    |            |  - tokio 网络 I/O            |
|  - Linux (iptables/nfqueue)|          |  - async-trait/thiserror     |
|  - macOS (NetworkExtension)|          |  - futures/stream combinators |
+---------------------------+            +-----------------------------+
```

### 数据流描述
1. **WFP Callout / 平台钩子** 在内核层截获新建连接，元数据(五元组、进程路径、域名 SNI)通过 RingBuffer/ALPC/IOCTL 传至用户态。
2. **用户态核心调度器** 使用函数式规则引擎对连接上下文进行匹配，生成路由决策（直连、TCP 上游、SOCKS5 上游、阻断）。
3. **透明代理管线** 基于 tokio 建立目标连接或上游代理隧道，并在两端之间执行零拷贝转发；支持 TCP、UDP（扩展占位）。
4. **用户界面层** 提供规则管理、实时日志、连接统计；CLI/GUI 共用同一服务接口（gRPC/local IPC）。

## 2. WFP Callout 与用户态交互
- **内核态 (C)**：
  - 注册 Callout (FWPM_LAYER_ALE_AUTH_CONNECT_V4/V6)。
  - 在 classifyFn 中收集流量元数据，写入锁自由环形缓冲区或 ALPC 消息队列。
  - 支持“旁路”模式：若用户态未响应，设置默认允许/拒绝策略。
- **用户态 (Rust, FFI)**：
  - 使用 `windows-sys` FFI 绑定加载驱动句柄，创建 ALPC 通道或 DeviceIoControl 与驱动通讯。
  - `PacketSource` trait 抽象事件流：`fn subscribe(&self) -> impl Stream<Item = ConnectionCtx>`。
  - Rust 侧接收后通过纯函数 `decide_route(ctx, rules)` 返回 `RouteAction`，再写回驱动（允许/阻断）或进入代理管线。
  - 失败降级：当规则评估或代理异常时，可回退直连或拒绝。

## 3. TCP / SOCKS5 上游代理实现
- **透明 TCP 代理**：
  - 通过 WFP 重定向或本地 TUN/NAT，将原始连接转发到本地监听端口。
  - `handle_tcp_flow(stream, route)` 以纯函数组合方式拼装：读/写 split -> `copy_bidirectional`。
- **SOCKS5 客户端**：
  - 使用异步状态机执行握手：`greet -> auth -> connect(request)`。
  - 支持用户名/密码认证与无认证，封装为 `socks5::connect_via(upstream, target)`。
- **上游选择**：
  - 路由决策包含 `ProxyKind::Direct | TcpForward(SocketAddr) | Socks5 { addr, auth }`。
  - 连接器通过 `enum` + `match` + `async fn` 组合，无共享可变状态，便于测试。

## 4. 模块职责与接口
### 核心模块 (`core`)
- **规则引擎**：
  - 数据类型：`Rule`, `RuleMatch`, `RuleSet`。
  - 组合器：`all_of`, `any_of`, `not`，使用迭代器/闭包实现。
  - 接口：`fn decide_route(ctx: &ConnectionCtx, rules: &RuleSet) -> RouteAction`。
- **连接调度**：
  - 接口：`async fn dispatch(ctx, action, io_provider)`，根据 action 选择直连或上游。
  - 使用 `Stream` 接口消费来自平台层的连接事件。
- **配置管理**：
  - `Config` 通过 serde + yaml 解析；校验生成规则集与上游表。

### 平台模块 (`platform::{windows, linux, macos}`)
- 提供统一 trait：`trait PacketSource { type Stream: Stream<Item = ConnectionCtx>; fn events(&self) -> Self::Stream; }`
- Windows 下包含：
  - `wfp_driver` (C) + `ffi` (Rust) + `callout` (注册/通讯封装)。
  - `classifier` 负责从驱动消息构造 `ConnectionCtx`。
- 其他平台预留：Linux 使用 `nfqueue/iptables`，macOS 使用 `NetworkExtension`。

### 用户交互模块 (`ui::{cli, gui}`)
- CLI：基于 `clap`，命令包括 `run`, `test-rule`, `list-processes`, `reload`。
- GUI：基于 `druid` 或 `gtk-rs`，MVC 模式；后台与核心通过 `tokio::mpsc` 通道或 gRPC 交互。

## 5. 函数式风格实现要点
- 使用不可变输入 + 纯函数生成决策；代理处理使用组合器与 small async 函数。
- 规则匹配、上游选择均避免全局状态，使用 `Arc<RuleSet>` 共享只读数据。
- 错误处理通过 `Result<T, ProxyError>` + `thiserror`，允许在管线中 `?` 传播。

## 6. 配置与规则示例 (YAML)
```yaml
upstreams:
  default_socks: { kind: socks5, addr: "127.0.0.1:1080" }
  datacenter: { kind: tcp, addr: "10.0.0.2:9000" }
rules:
  - name: block_malware
    match: { domain: ["*.malicious.com"], process: ["C:/bad.exe"] }
    action: { block: true }
  - name: internal
    match: { ip_cidr: ["10.0.0.0/8"] }
    action: { upstream: "datacenter" }
  - name: default
    match: { any: true }
    action: { upstream: "default_socks" }
```

## 7. 单元测试范围 (自动生成占位)
- 规则匹配：IP/域名/进程路径；组合器 truth table。
- SOCKS5 握手：无认证/用户名密码；失败分支。
- 调度器：直连 vs 上游 vs 阻断 的选择。
- 配置解析：YAML -> Config -> RuleSet 校验。

## 8. 可扩展性与维护性
- 平台特性隔离在 `platform::*`，核心逻辑与 UI 可跨平台复用。
- 规则/配置使用声明式数据结构，便于未来支持 gRPC 控制面、热更新。
- 模块通过 trait + 组合器解耦，便于替换上游协议（如 HTTP CONNECT、QUIC）。
