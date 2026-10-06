use mirui::core::model::BindingOwner;

fn main() {
    let owner = BindingOwner::unbound();
    let _ = owner.0;
    let _ = format!("{owner:?}");
}
