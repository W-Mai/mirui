# Build a task list

<script setup>
import { useData } from 'vitepress';
const { theme } = useData();
</script>

Start with one window and turn it into an interactive, animated task list with persistence. The seven chapters share one project and each has a runnable checkpoint.

- [Open a window](window.md)
- [Complete a task](state.md)
- [Add, remove, and filter](collections.md)
- [Reuse a component and its styles](components.md)
- [Give the interface motion](motion.md)
- [Save and restore the list](storage.md)
- [Run and verify it in the browser](web.md)

Try each chapter in the browser. Install Rust and SDL2 to edit and run the same code locally.

Clone the repository and check out the source used by this manual.

<pre><code>git clone https://github.com/W-Mai/mirui.git
cd mirui
git checkout {{ theme.source }}</code></pre>
