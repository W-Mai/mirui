# 动手做一个任务清单

<script setup>
import { useData } from 'vitepress';
const { theme } = useData();
</script>

从一个窗口开始，逐步把它做成可操作、有动效、能保存的应用。七章使用同一个教程工程。每一步有可以运行的检查点。

- [打开窗口](window.md)
- [完成一条任务](state.md)
- [增删和筛选任务](collections.md)
- [复用组件和样式](components.md)
- [给应用加动效](motion.md)
- [保存和恢复任务](storage.md)
- [在浏览器运行并验证](web.md)

在浏览器里可以直接动手。要在本机修改并运行这些代码，请安装 Rust 和 SDL2。

克隆仓库并切换到这份手册对应的源码。

<pre><code>git clone https://github.com/W-Mai/mirui.git
cd mirui
git checkout {{ theme.source }}</code></pre>
