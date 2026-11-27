# CI 检查与门禁 {#ci-checks}

`cargo xtask ci` 统一驱动全仓库的质量门禁流水线——涵盖代码格式校验、`cha` 架构静态分析、双配置运行时测试（`std` + `linux-fb`）、`trybuild` 编译期诊断契约以及裸机/WebAssembly 交叉目标检查。

<ArchFlow kind="ci-matrix" />

## 在本地运行 CI 检查 {#running-ci-checks-locally}

使用 `cargo xtask ci --list` 列出所有已注册的检查项，或传入指定名称仅运行对应的门禁子集：

```sh
cargo xtask ci format
cargo xtask ci cha-policy cha-analysis
cargo xtask ci tests-std
cargo xtask ci contracts-std
cargo xtask ci host-clippy examples
cargo xtask ci wasm-check riscv-check linux-fb-check
```

检查项名称在执行前会先行校验：未知或重复的名称会直接报错终止，绝不会静默回退到其他检查项。每项检查都会输出墙钟耗时（包含子进程执行与系统调度耗时）。

在本地开发环境中，若缺少交叉编译目标或非 Linux 宿主环境，相关检查可自动跳过；而在 GitHub Actions 中，任何缺失目标工具链或被跳过的检查都会直接判负。

在 macOS 上，运行文档测试（doctest）与原生构建一样需要指定 SDL 库搜索路径。对于 Apple Silicon + Homebrew 环境，请执行：

```sh
env RUSTDOCFLAGS="-L native=/opt/homebrew/lib" cargo xtask ci tests-std
```

---

## 检查分组与运行器矩阵 {#check-groups-and-runner-matrix}

GitHub Actions 从 `.github/ci-matrix.json` 读取检查分组，并在独立运行器（runner）上并发执行最多 6 个分组。每个分组使用独立的 Rust 缓存键，并按需安装专属工具链组件（`rustfmt`、`clippy`、`xrune-fmt`、`cha` 以及 `wasm32` / `riscv32imc` 目标标准库）。

| 矩阵分组 | `cargo xtask ci` 检查项 | 核心职责与前置条件 |
| --- | --- | --- |
| **代码与样式表格式化** | `format` | 校验全仓 Rust 代码的 `cargo fmt --check` 与 `.xrune` 样式表的 `xrune-fmt` |
| **Cha 策略与静态分析** | `cha-policy`, `cha-analysis` | 先校验健康度夹具（必须触发预期诊断错误），再执行仓库 API 边界与架构规则检查（本地可用 `cargo xtask cha` 一并运行） |
| **运行时测试集 (`std` & `linux-fb`)** | `tests-std`, `tests-linux-fb` | 在标准配置与 Linux 帧缓冲配置下分别运行库/二进制单元测试、自动发现的运行时集成测试与文档测试 |
| **编译器契约测试 (`std` & `linux-fb`)** | `contracts-std`, `contracts-linux-fb` | 在锁定 `rustc 1.97.0` 工具链下运行 `[package.metadata.ci].contract-tests` 声明的 `trybuild` 编译失败 `.stderr` 快照与外部可见性契约 |
| **宿主 Lint、Release 与体积门禁** | `host-clippy`, `examples`, `release`, `size` | 宿主零警告 Clippy 检查、Gallery 与示例编译、Release 优化构建与产物体积检查 |
| **跨平台与裸机交叉目标** | `wasm-check`, `riscv-check`, `linux-fb-check` | 校验 `wasm32-unknown-unknown`、裸机 `riscv32imc-unknown-none-elf`（`no_std + alloc`）与 Linux `fbdev`/`evdev` 目标 |

### 集成测试自动发现与渲染器单测 {#test-discovery-and-renderer-unit-suites}

集成测试目标通过 Cargo 元数据自动归类：
- 各软件包在 `[package.metadata.ci].contract-tests` 中显式列出编译期契约测试目标；
- 其余所有集成测试目标自动进入运行时测试组——新增普通集成测试**无需修改任何 CI 列表配置**；
- 渲染器内部单元测试位于 `tests/unit/render/` 下，通过模块挂载保留对私有渲染类型的访问权限。可单独运行：

```sh
cargo test --lib --features sdl-gpu,wgpu render::
```

最终的 `ci` 汇总门禁要求矩阵准备阶段与所有检查分组全部成功完结；任何缺失、跳过、取消或失败的分项都会阻断合入。

---

## 文档站点检查 {#documentation-checks}

在 `docs/` 目录下使用 `bun install --frozen-lockfile` 安装锁定依赖，构建编译级 Wasm 演练场，并执行静态站点构建与 20 篇双语指南 1:1 锚点对齐测试：

```sh
bun install --frozen-lockfile
bun run playground
bun run build
bun run test
cargo test -p mirui-first-app
```

GitHub Pages 工作流在独立运行器上分别构建 Gallery 展厅与双语文档手册。当 `mirui-first-app` 针对已发布的 crate 完成构建且 Pages 校验归档哈希与 Wasm 产物通过后，对应的版本快照即会出现在顶栏版本切换器中。
