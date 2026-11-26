# CI 检查与门禁 {#ci-checks}

`cargo xtask ci` 运行完整的门禁检查集。使用 `cargo xtask ci --list` 可以列出所有检查项，也可以直接传入检查项名称以仅运行指定门禁：

```sh
cargo xtask ci format
cargo xtask ci cha-policy cha-analysis
cargo xtask ci tests-std
cargo xtask ci contracts-std
cargo xtask ci host-clippy examples
cargo xtask ci wasm-check riscv-check linux-fb-check
```

检查项名称会在执行前先行校验。未知或重复的名称会直接报错终止，而不会静默回退到其他检查项。每项检查都会汇报经过的墙钟耗时（wall time），其中包含子进程执行与操作系统调度耗时。

在本地开发环境中，当交叉编译目标或仅限 Linux 的检查缺少前置条件时可以自动跳过。而在 GitHub Actions 中，一旦选中的检查项缺少目标工具链或当前宿主不受支持，任务会直接判定失败，绝不会将跳过伪装成通过。

`tests-std` 运行工作区库与二进制目标的单元测试、运行时集成测试以及文档测试（doctest）。`contracts-std` 运行编译器诊断契约与外部 crate 契约测试。对应的 `tests-linux-fb` 与 `contracts-linux-fb` 检查项保留 Linux 帧缓冲（framebuffer）特性配置，并在 Linux 宿主上执行。

渲染器单元测试位于 `tests/unit/render/` 下，通过原有的模块声明继续访问私有渲染类型。可使用 `cargo test --lib --features sdl-gpu,wgpu render::` 单独运行渲染器测试集。Pixel Flight 与 SpriteStack 各自将单元测试保留在其 crate 内部的 `tests/unit/` 目录中。

集成测试目标通过 Cargo 元数据自动发现。各软件包在 `[package.metadata.ci].contract-tests` 中声明编译器契约测试目标；其余所有集成测试目标都会自动归入运行时测试组。声明过时或工作区目标名称存在歧义都会导致检查失败。新增普通集成测试无需手动更新 CI 列表。

各测试组使用与全工作区测试完全一致的特性（feature）集合。将编译器诊断契约拆分到独立测试组并不会禁用它们，也不会将其降级为源码文本匹配断言。原生剪贴板测试二进制文件保留各自独立的测试驱动（harness）行为。

在 macOS 上，链接文档测试与原生 Rust 构建一样需要指定 SDL 库搜索路径。对于 Apple Silicon 上的 Homebrew 环境，请使用 `env RUSTDOCFLAGS="-L native=/opt/homebrew/lib" cargo xtask ci tests-std`。

Cha 检查会分别汇报每个健康度测试夹具（health fixture）、仓库 API 策略以及源码静态分析的耗时。健康度夹具必须触发预期的诊断错误；仓库级检查则严格维持既定的错误阈值与缓存策略。检查失败时会直接打印已捕获的 JSON 与标准错误输出，而不会重复跑一遍分析过程。

GitHub Actions 从 `.github/ci-matrix.json` 读取检查分组，并在不同运行器（runner）上并发执行最多六个分组。代码格式化、API 策略、通用静态分析、Release 构建、宿主 Lint/示例、运行时测试、编译器契约以及交叉目标检查各自独立汇报结果。每套编译器契约与运行时测试套件都会在标准配置和 Linux 帧缓冲配置下各跑一遍。两个 Cha 分组在执行正式分析前都会先校验分析器本身与健康度夹具；本地运行 `cargo xtask cha` 则会一并执行这两项检查。

不同分组使用各自独立的 Rust 缓存键，并按需安装所需的原生依赖库与工具。构建与二进制体积检查共用同一台运行器，宿主 Clippy 与示例检查也共用运行器。同一分组内的 Cargo 命令按顺序串行执行，以复用该运行器上的 `target` 目录。工作流会显式选择锁定的 Rust 工具链版本，并为交叉目标检查组声明所需的跨平台组件。

工具链组件作为矩阵的显式前置条件声明：格式化分组安装 `rustfmt`，宿主 Lint 与 RV32 检查组安装 `clippy`。格式化分组还会安装 `xrune-fmt`；两个静态分析组安装 Cha。原生构建与测试组安装 SDL 与 evdev 开发库。交叉目标检查组安装 WASM 与 RV32 标准库目标。矩阵覆盖率测试会校验这些依赖声明是否完整，并验证它们是否被正确转发给工具链安装脚本。

最终的 `ci` 汇总任务要求矩阵准备阶段成功，且每一个检查组都成功完结。任何缺失、跳过、取消或失败的分组结果都无法生成成功的汇总状态。架构契约测试会校验矩阵是否不重不漏地恰好覆盖每一个已注册检查项，并针对各种结果组合模拟执行汇总脚本。发布工具（release tool）同样会等待对应精确提交上的完整工作流执行完毕。

并行任务能够缩短串行关键路径，但可能会增加总运行器工时与缓存体积。新拆分的分组缓存起初可能是冷缓存；在宣称提速之前，应在同等条件下对比完整工作流的端到端耗时。有关运行器并发与故障处理的更多细节，请参阅 GitHub 的[矩阵执行文档](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/run-job-variations)。

## 文档站点检查 {#documentation-checks}

在 `docs/` 目录下，使用 `bun install --frozen-lockfile` 安装锁定版本的依赖。运行 `bun run playground` 构建可选的浏览器交互应用，随后通过 `bun run dev` 启动本地预览站点。开发版手册挂载在 `/docs/dev/` 路径下。

使用 `bun run test` 运行路由与版本归档检查，使用 `cargo test -p mirui-first-app` 验证互动课程的各个章节检查点，并使用 `bun run build` 执行静态站点构建与死链校验。GitHub Pages 工作流会在独立的运行器上分别构建 Gallery 展厅与文档手册。当课程针对已发布的 crate 完成构建且 Pages 验证快照无误后，对应的发布快照就会出现在文档站点的版本切换器中。
