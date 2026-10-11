# Composing text editors

`TextDocument` stores the UTF-8 content and its byte capacity. `TextEditing` binds that document to one editor's selection, focus, and input handling. `TextInput` draws the editor. Custom components can combine these capabilities on the same entity without accessing World or implementing their own keyboard handling.

## A custom search editor

This example owns the query in an application Model. `SearchEditor()` attaches the editing capability and standard appearance; a second `TextInput()` shares the query while keeping its own selection. Return invokes the application's submit method through the existing notification API.

```rust
use mirui::prelude::*;
use mirui::ui::widgets::{
    Placeholder, TextDocument, TextEditing, TextEditError, TextInput,
    text_editing, text_input,
};

#[model]
struct SearchForm {
    query: model!(owned TextDocument),
    #[observe]
    submissions: u32,
}

impl Default for SearchForm {
    fn default() -> Self {
        Self {
            query: TextDocument::new().into(),
            submissions: 0,
        }
    }
}

#[model]
impl SearchForm {
    fn submit(&mut self) {
        self.submissions += 1;
    }
}

#[component]
#[derive(Default)]
struct SearchEditor {
    document: model!(optional TextDocument),
}

#[event]
fn attach_search(
    node: entity!(),
    search: query!(SearchEditor),
    editing: query!(TextEditing),
    appearance: query!(TextInput),
    commands: commands!(),
) {
    let Some(search) = search.get(node) else {
        return;
    };
    if editing.get(node).is_none() {
        let editor = search.document.clone()
            .map(TextEditing::new)
            .unwrap_or_default();
        commands.insert(node, editor);
    }
    if appearance.get(node).is_none() {
        commands.insert(node, TextInput::new());
    }
}

#[view(component = SearchEditor, attach = attach_search::callback())]
fn search_view() {}

#[compose]
fn panel(form: model!(SearchForm)) {
    ui! {
        Column(row_gap: 12) {
            SearchEditor(document: Some(form.query()), width: 240, height: 36) [
                Placeholder("Search"),
            ] on TextEditing::Submitted {
                form.submit();
            }
            TextInput(width: 240, height: 36) [
                TextEditing::new(form.query()),
            ]
        }
    };
}

# fn main() -> Result<(), TextEditError> {
let mut app = App::headless(320, 160);
app.with_widget(search_view::view())
    .with_widget(text_input::view())
    .with_widget(text_editing::view());
let root = app.spawn_root().id();
let form = app.add_model(SearchForm::default());
form.query().set_capacity(256)?;
form.query().set_text("Canvas")?;
app.compose(root, panel(form.clone()));

form.query().replace(0..6, "Components")?;
form.query().read(|text| assert_eq!(text, "Components"));
# Ok(())
# }
```

Registering a View installs its behavior; inserting a Component supplies per-node state. These are separate operations. `App::with_default_widgets()` already registers TextInput and TextEditing, so an application using that preset only adds its custom View. The empty `search_view` body declares no paint hook. Its attachment preserves explicitly supplied editing and appearance components instead of overwriting them.

`SearchEditor()` without a document creates a local document with 32 inline bytes. For a larger local buffer, attach `TextEditing::default().with_capacity(256)?` explicitly when composing the node. For a shared document, prepare capacity on the registered document before composing its editors. Capacity is a UTF-8 byte limit, not a character count; rejected edits leave the content unchanged. Capacity preparation may allocate, while replacements within prepared storage reuse that storage.

Programmatic changes go through `form.query()` and publish to its observers. They do not synthesize `Changed` or `Submitted` input notifications. Each editor keeps its own UTF-8 selection; external changes clamp invalid positions rather than moving them relative to an edit history. To derive application values from the query, observe its `revision()` and borrow the content with `read`.

The `document` field supplies the binding when the custom component is attached. Replacing that field alone is not a request to rebind an already attached TextEditing; replace the TextEditing component through `commands.insert` when changing an existing editor's source.

See [Widget input and notifications](https://github.com/W-Mai/mirui/blob/main/docs/widget-events.md) for accepted input and notification ordering. Gallery's [Editor Lab](https://github.com/W-Mai/mirui/tree/main/src/gallery/demos/editor_lab) includes search filtering, command submission, and shared-document editing.
