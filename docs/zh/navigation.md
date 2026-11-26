# 页面导航与物理动效 {#navigation-paths}

## 跟随控件几何边界 (`FollowBounds`) {#following-widget-bounds}

将 `FollowBounds` 挂载到任意控件上，即可让它自动环绕定位到另一个目标控件周围。它直接读取布局阶段准备好的 `LayoutGeometry` 输出（包含滚动偏移与几何变换），并将目标矩形映射到跟随者父容器的局部坐标系中。跟随者自身不占用 Flex 布局空间，而其内部子节点依然遵循常规的布局与内边距规则。

<MiruiSnippet recipe="spring" label="FollowBounds" file="recipes.rs" height="240px">

<<< @/examples/first-app/src/recipes.rs#recipe-spring{rust}

<template #exercise>

在 `MCU`、`Desktop` 与 `Web` 三个不同宽度的按钮间快速连点，观察 `FollowBounds` 如何在半途带着当前速度平滑折返并自适应目标宽度。

</template>

</MiruiSnippet>

```rust
use mirui::prelude::*;
use mirui::ui::widgets::FollowBounds;

#[compose]
fn selection() {
    ui! {
        View (width: 240, height: 120) {
            View (id: "selected", width: 80, height: 40, bg_color: Color::rgb(30, 70, 120))
            View (border_width: 2, border_color: Color::rgb(80, 160, 240)) [
                FollowBounds::new(id("selected")).with_outset(3),
            ]
        }
    };
}

let mut app = App::headless(240, 120);
app.with_default_widgets();
let root = app.spawn_root().id();
app.compose(root, selection());
app.render().unwrap();
```

- `with_content(true)`：跟随目标扣除内边距（padding）后的内容框；
- `with_outset(value)`：向外扩张映射后的矩形（传入负数则向内收缩）；
- `with_offset(x, y)`：在跟随者父容器的逻辑坐标系中附加平移偏移。若目标存在旋转，会映射为其轴对齐外接矩形（AABB）。

跟随者同时遵循自身祖先与目标祖先的裁剪区域。当目标处于隐藏、已删除、未挂载、循环引用（如指向自身或自身子树内部）或无法映射的状态时，跟随者会自动暂停绘制、输入与输出读取，直到目标关系重新合法；移除 `FollowBounds` 组件则会恢复控件原本书写的 `Style`。

默认情况下边界立即跳变。加上 `.with_transition(mirui::anim::SNAPPY)` 或传入 `TweenConfig` 即可为位置与尺寸变化开启平滑动画。首次挂载定位与从不可用状态恢复时总是立即到位；在运动途中切换目标（retargeting）会保留各轴当前的显示位置，弹簧过渡还会**保留当前运动速度**。只有启用了过渡动画的跟随者才会分配运动组件存储，并在后续推进与重定向时原地复用。

## 交互式演示 (`page_studio` 与 `nearby`) {#interactive-example}

Gallery 的 Showcase 展区提供了两个完整的导航与动效示例：

1. **Page Studio (`page_studio`)**：通过收藏夹、地点详情与笔记编辑器演示保留状态的栈式导航（`NavigationStack`），通过场景面板演示受状态驱动的单页切换（`PageView`），并在工具栏中提供 `None / Slide / Fade` 视觉效果与 `Tween / Spring`（`Smooth`、`Snappy`、`Bouncy`、`Interactive`）运动算法的实时正交组合对比：

```sh
cargo run -p gallery --example page_studio_demo
cargo run -p gallery --example page_studio_demo --no-default-features --features wgpu
```

2. **Nearby (`nearby_demo`)**：本地掌机游戏启动器，包含收藏夹、平台与游戏目录浏览、搜索键盘、多层模态弹窗、十种内置音效以及联机房间发现流程（按 `F2` 或点击 Web 画布上方的 `Simulate` 可展开左侧 300 逻辑像素宽的联机状态机注入检查器）：

```sh
cargo run -p gallery --example nearby_demo
cargo run -p gallery --example nearby_demo --no-default-features --features sdl-gpu
cargo run -p gallery --example nearby_demo --no-default-features --features wgpu
```

## 导航路径状态 (`NavigationPath<R>`) {#navigation-state}

`NavigationPath<R>` 是一个存放已压入路由目标（pushed destinations）的可观察模型。根页面本身不包含在路径栈内。可通过 `App::add_model` 独立注册，或作为 `model!(owned NavigationPath<R>)` 嵌入父模型：

```rust
use mirui::app::App;
use mirui::ui::widgets::navigation::NavigationPath;

#[derive(Clone)]
enum Route {
    Devices,
    Device(u32),
}

let mut app = App::headless(320, 240);
let path = app
    .add_model(NavigationPath::<Route>::with_capacity(8))
    .finish();

path.push(Route::Devices).unwrap();
path.push(Route::Device(7)).unwrap();
assert_eq!(path.len(), 2);
path.pop().unwrap();
assert_eq!(path.len(), 1);
path.clear().unwrap();
assert!(path.is_empty());
```

`len()`、`is_empty()` 与 `revision()` 均可被响应式观察。`with_capacity(n)` 预先准备最多容纳 `n` 层路由的存储槽位：`push` 直接将路由值移入预备槽位（不扩容、不克隆）；`pop` 与 `clear` 保留已分配容量；对空栈调用 `pop` 不会发布多余的变更通知。

## 保留状态的导航栈 (`NavigationStack`) {#retained-navigation-stacks}

`NavigationStack` 将 `NavigationPath<R>` 绑定到页面生成回调 `on CreatePage`。它的直接子节点就是根页面（Root Page），而每当路径压入新路由时則调用 `CreatePage` 构造子页面。每种路由枚举类型 `R` 只需调用一次 `app.with_widget(navigation::view::<Route>())` 注册视图：

<MiruiSnippet recipe="nav-stack" label="NavigationStack" file="recipes.rs" height="220px">

<template #exercise>

点击 `Push Devices →` 与 `Inspect #7 →` 连续压入两层路由，再点 `← Pop one` 或 `Clear root` 滑回上一层。

</template>

</MiruiSnippet>

```rust
use mirui::prelude::*;
use mirui::ui::widgets::{Button, Text};
use mirui::ui::widgets::navigation::{self, CreatePage, NavigationPath, NavigationStack, PageTransition};
use mirui::anim::SMOOTH;

#[derive(Clone, Copy)]
enum Route { Devices, Device(u32), Settings }

#[compose]
fn shell(path: model!(NavigationPath<Route>)) -> Entity {
    ui! {
        NavigationStack(path: path.clone(), grow: 1.0,
            transition: PageTransition::slide(SMOOTH)) {
            Column(grow: 1.0) {
                Text("Home")
                Button(height: 40) [Text::label("Devices")]
                on Tap { path.push(Route::Devices).unwrap(); }
                Button(height: 40) [Text::label("Settings")]
                on Tap { path.push(Route::Settings).unwrap(); }
            }
        } on CreatePage(create_page::callback(path))
    }
}

#[event]
fn create_page(event: event!(CreatePage<Route>), path: model!(NavigationPath<Route>)) -> Entity {
    match *event.route() {
        Route::Devices => ui! { devices(path) },
        Route::Device(id) => ui! { detail(path, id) },
        Route::Settings => ui! { settings(path) },
    }
}
```

当从栈顶弹出返回到此前访问过的页面时，**原有的控件树、输入框状态与滚动位置均原样恢复**。处于栈底隐藏状态的页面不接收输入，并在重新显示时自动恢复响应式更新；而被弹出栈外的条目则会释放其页面子树。一个 `NavigationPath` 实例同一时刻只能驱动一个活动的 `NavigationStack`。

## 受应用状态直接驱动的单页视图 (`PageView`) {#pages-selected-by-application-state}

如果不需要维护前进/后退历史栈，而是希望直接根据某个模型字段切换当前展示的页面，请使用 `PageView`：

<MiruiSnippet recipe="nav-pageview" label="PageView" file="recipes.rs" height="210px">

<template #exercise>

点击 `Overview`、`Metrics` 或 `Logs` 修改 `session.page()`，驱动 `PageView` 切换当前子页面。

</template>

</MiruiSnippet>

```rust
use mirui::prelude::*;
use mirui::ui::widgets::Text;
use mirui::ui::widgets::page_view::{self, CreatePage, PageView};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page { Home, Settings }

#[model]
struct Session {
    #[observe]
    page: Page,
}

#[model]
impl Session {
    fn show_settings(&mut self) { self.page = Page::Settings; }
}

#[event]
fn create_page(event: event!(CreatePage<Page>)) -> Entity {
    match event.route() {
        Page::Home => ui! { Text("Home", grow: 1.0) },
        Page::Settings => ui! { Text("Settings", grow: 1.0) },
    }
}

#[compose]
fn panel(session: model!(Session)) -> Entity {
    ui! {
        PageView(page: ${ session.page() }, grow: 1.0)
        on CreatePage(create_page::callback())
    }
}
```

路由值只需实现 `Clone + PartialEq`。当绑定值未发生变化时不会克隆路由或重建子树；当值改变时，会先构造新页面子树，再退役旧页面子树。

## 页面转场动画 (`PageTransition`) {#page-transitions}

```rust
use mirui::anim::{BOUNCY, Easing, Transition, TweenConfig, ease};
use mirui::types::Fixed;
use mirui::ui::widgets::navigation::{PageDirection, PageTransition};

const MOVE: TweenConfig = TweenConfig::new(300, &ease::OUT_CUBIC);
let slide = PageTransition::slide(MOVE);
let returning = PageTransition::slide(MOVE).with_direction(PageDirection::Backward);
let fade = PageTransition::fade(BOUNCY);

fn custom_progress(t: Fixed) -> Fixed { t * t }
static CUSTOM: Easing = Easing::new(custom_progress);
let config: Transition = TweenConfig::new(240, &CUSTOM).into();
let mut motion = config.motion(Fixed::ZERO, Fixed::ONE);
motion.tick(16);
config.retarget(&mut motion, Fixed::from_int(2));
```

`NavigationStack` 与 `PageView` 均支持 `transition: PageTransition::slide(config)`（水平推拉）或 `PageTransition::fade(config)`（交叉淡入淡出），并可传入任意 `TweenConfig` 或弹簧预设（`SMOOTH`、`SNAPPY`、`BOUNCY`、`INTERACTIVE`）。`NavigationStack` 在压栈（push）时自动向前滑入、弹栈（pop）时自动反向滑出；`PageView` 默认向前滑动，也可通过 `.with_direction(PageDirection::Backward)` 显式指定反向滑入。

- **中途折返（Reversal）**：在两个页面正在转场途中立即切回上一页时，不会销毁重建任一页面，而是直接将正在进行的运动平滑反向（弹簧保留当前速度）；转场进行期间新旧两个页面均暂停接收指针输入，待转场落定后立即恢复。
- **离屏缓冲开销**：`slide` 推拉转场**无需额外像素缓冲**；`fade` 淡入淡出则复用 `app.with_offscreen_pool_budget(bytes)` 配置的离屏缓冲池合成两棵子树（1 倍缩放下两张 `W × H` RGBA 缓冲占用 `8 × W × H` 字节；若预算不足则记录警告并直接完成无动画切换，绝不隐式超额分配）。
