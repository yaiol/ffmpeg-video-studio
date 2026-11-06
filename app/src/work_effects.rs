//! The work view's effects: the STANDARD effects of the catalogue as rows that are always there, each with a switch and a
//! title (a click on the title opens its form), in a fixed order; and under them every other effect, added from the same
//! menu as Settings > Effects and shown as a card that can be removed. The list is the work view's own, for the file in
//! front of it: it is not a saved effects entry (those are the batch view's), and the preview shows it as it stands.
//!
//! ⚠ CLAUDE: NO HARDCODED UI STRINGS. Every user-visible string goes through the t() lookup with the key
//! a string literal at the call site.

use crate::{
    catalog::{UisCboCategory, UisComboItem, UisCboFilter},
    effects_editor::{SYSTEM, form_text, menu_of, menu_rows_of, named},
    effects_form::{BrowseRequest, CatalogRef, EffectForm},
    i18n,
};
use crate::work::Mixed;
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdChevronDown, LdChevronRight, LdListFilter, LdTrash2},
};
use engine::{
    effects::{self, EffectsEntry, Item},
    ffmpeg_effects::{V, Values},
};
use std::collections::{HashMap, HashSet};

/// The categories of the catalogue in its order (id, title), and the category of every effect the add menu offers.
fn category_index(sound: bool) -> (Vec<(String, String)>, HashMap<String, String>) {
    let Some(cat) = effects::catalog() else { return Default::default() };
    let entries = if sound { cat.picker_for(SYSTEM, "audio", true) } else { cat.picker(SYSTEM, effects::WORKS) };
    (cat.categories.iter().map(|c| (c.id.clone(), c.title.clone())).collect(), entries.into_iter().map(|e| (e.key, e.category)).collect())
}

/// One category of the list: the standard effects that belong to it (always there, a switch each), and the added ones.
struct Group {
    id: String,
    title: String,
    /// How many of its effects are in the list now.
    on: usize,
    rows: Vec<(String, Option<(usize, Item)>)>,
    extras: Vec<(usize, Item)>,
}

/// The group of a category, made when it is not there yet.
fn group_of<'a>(groups: &'a mut Vec<Group>, id: &str) -> &'a mut Group {
    let at = groups.iter().position(|g| g.id == id).unwrap_or_else(|| {
        groups.push(Group { id: id.to_owned(), title: id.to_owned(), on: 0, rows: Vec::new(), extras: Vec::new() });
        groups.len() - 1
    });
    &mut groups[at]
}

/// The standard effects the installed catalogue offers, in the fixed order.
pub(crate) fn standard() -> Vec<String> {
    effects::catalog().map(|c| effects::standard_keys(&c, SYSTEM)).unwrap_or_default()
}

/// The standard effects of the sound (FFmpeg Audio Studio's whole list): the ones with a widget are a drawer or a strip on the preview, the others
/// are rows with a switch like the picture's; the add menu does not offer any of them.
pub(crate) fn sound_standard() -> Vec<String> {
    effects::catalog().map(|c| effects::sound_standard_keys(&c, SYSTEM)).unwrap_or_default()
}

/// Every standard effect of the list, the picture's and the sound's: the list keeps them first, in their fixed order.
pub(crate) fn arranged_keys() -> Vec<String> {
    let mut keys = standard();
    keys.extend(sound_standard());
    keys
}

/// The keys of the sound's standard effects drawn by a drawer or a strip of their own (the level, the tone, the compressor, the channels, the
/// fade): not rows, and the add menu does not offer them.
pub(crate) fn sound_widget_keys() -> Vec<String> {
    effects::catalog().map(|c| effects::sound_widgets(&c, SYSTEM).into_iter().map(|w| w.key).collect()).unwrap_or_default()
}

/// The keys of the standard effects drawn by a widget over the picture: they are not rows, and the add menu does not offer them.
pub(crate) fn widget_keys() -> Vec<String> {
    effects::catalog().map(|c| effects::widgets(&c, SYSTEM).into_iter().map(|w| w.key).collect()).unwrap_or_default()
}

/// The rows of the add menu without the standard effects - each is already in the interface, as a row of the list or as a widget over the
/// picture - and without a heading left with nothing under it.
fn work_menu_rows(sound: bool, query: &str, category: &str) -> Vec<(String, String, bool)> {
    // the sound's standard effects are in the interface already (a row, a drawer or a strip): everything else it offers is in the menu
    let hidden = if sound { sound_standard() } else { standard() };
    let mut rows: Vec<(String, String, bool)> = Vec::new();
    for row in menu_rows_of(sound, query, category) {
        if !row.2 && hidden.contains(&row.0) {
            continue;
        }
        if row.2 && rows.last().is_some_and(|r| r.2) {
            rows.pop();
        }
        rows.push(row);
    }
    if rows.last().is_some_and(|r| r.2) {
        rows.pop();
    }
    rows
}

/// Apply `edit` to the list and keep it in its shape: the standard effects first, in their order.
fn edit(mut list: Signal<EffectsEntry>, standard: &[String], edit: impl FnOnce(&mut EffectsEntry)) {
    let mut e = list.peek().clone();
    edit(&mut e);
    list.set(e.normalized().arranged(standard));
}

/// What ONE effect shows: its title, and its form when it is open.
#[component]
fn Row(title: String, remove: String, switch: Option<bool>, open: bool, on_switch: EventHandler<bool>, on_toggle: EventHandler<()>, on_remove: EventHandler<()>, #[props(default)] partial: bool, #[props(default)] differs: bool, #[props(default)] children: Element) -> Element {
    // a checkbox's third state is a property, not an attribute: set from here
    use_effect(use_reactive!(|partial| {
        let _ = partial;
        let _ = document::eval("document.querySelectorAll('input[data-mixed]').forEach(e => { e.indeterminate = e.dataset.mixed === 'true'; });");
    }));
    rsx! {
        div { class: "fx-card wfx-row",
            div { class: "fx-head",
                if let Some(on) = switch {
                    input {
                        r#type: "checkbox",
                        title: "{title}",
                        "aria-label": "{title}",
                        checked: on && !partial,
                        "data-mixed": "{partial}",
                        onchange: move |e| on_switch.call(e.checked()),
                    }
                }
                button { class: "fx-title", r#type: "button", "aria-expanded": "{open}", onclick: move |_| on_toggle.call(()),
                    if open {
                        Icon { width: 12, height: 12, icon: LdChevronDown }
                    } else {
                        Icon { width: 12, height: 12, icon: LdChevronRight }
                    }
                    span { class: "wfx-title", "{title}" }
                    if differs {
                        span { class: "wfx-mixed", "≠" }
                    }
                }
                if switch.is_none() {
                    button {
                        class: "uis-btn ico",
                        r#type: "button",
                        title: "{remove}",
                        "aria-label": "{remove}",
                        onclick: move |_| on_remove.call(()),
                        Icon { width: 12, height: 12, icon: LdTrash2 }
                    }
                }
            }
            if open {
                div { class: "fx-body wfx-body", {children} }
            }
        }
    }
}

/// The form of the effect at `at` in the list, written back at once.
#[component]
fn Form(list: Signal<EffectsEntry>, lang: String, at: usize, item: Item, mixed: HashSet<String>, touched: Signal<HashSet<String>>) -> Element {
    let Some(cat) = effects::catalog() else { return rsx! {} };
    let standard = arranged_keys();
    let (std_set, std_browse) = (standard.clone(), standard);
    let browse_item = item.clone();
    // Where the selected files disagree a control shows blank; a touch of it is told to the panel, which copies it to every file.
    let blank: std::collections::BTreeSet<String> = item.all_values().keys().filter(|id| mixed.contains(&format!("{}.{id}", item.key))).cloned().collect();
    let key = item.key.clone();
    rsx! {
        EffectForm {
            catalog: CatalogRef(cat),
            effect: item.key.clone(),
            values: item.all_values(),
            mixed: blank,
            on_touch: move |id: String| {
                touched.write().insert(format!("{key}.{id}"));
            },
            text: form_text(&lang),
            named: named(),
            compact: true,
            on_change: move |v: Values| edit(list, &std_set, |e| {
                if let Some(x) = e.items.get_mut(at) {
                    x.set_values(&v);
                }
            }),
            on_browse: move |req: BrowseRequest| {
                let (item, std) = (browse_item.clone(), std_browse.clone());
                spawn(async move {
                    let picked = match req.kind.as_str() {
                        "folder" => rfd::AsyncFileDialog::new().pick_folder().await,
                        "saveFile" => rfd::AsyncFileDialog::new().save_file().await,
                        _ => rfd::AsyncFileDialog::new().pick_file().await,
                    };
                    if let Some(f) = picked {
                        let mut values = item.all_values();
                        values.insert(req.id.clone(), V::Str(f.path().to_string_lossy().into_owned()));
                        edit(list, &std, |e| {
                            if let Some(x) = e.items.get_mut(at) {
                                x.set_values(&values);
                            }
                        });
                    }
                });
            },
        }
    }
}

/// How the effects list looks: which rows are open and what the add menu is showing. Kept by the app, so it is as it was
/// when the view comes back.
#[derive(Clone, Copy, PartialEq)]
pub struct FxView {
    /// The rows that are open: a standard effect by its key, an added one by its place in the list ("x" and the number).
    open: Signal<HashSet<String>>,
    query: Signal<String>,
    category: Signal<String>,
}

impl FxView {
    /// A hook: call it from a component that outlives the view.
    pub fn new() -> FxView {
        FxView { open: use_signal(HashSet::new), query: use_signal(String::new), category: use_signal(String::new) }
    }
}

#[component]
pub fn WorkEffects(list: Signal<EffectsEntry>, view: FxView, lang: String, mixed: Mixed, touched: Signal<HashSet<String>>, #[props(default)] sound: bool) -> Element {
    let t = |key: &str| i18n::t(&lang, key);
    let FxView { mut open, mut query, mut category } = view;

    // The list holds the picture's effects and the sound's together; this block shows one kind.
    let standard = arranged_keys();
    let (rows_std, widgets) = if sound { (sound_standard(), sound_widget_keys()) } else { (self::standard(), widget_keys()) };
    let wfx_category = if sound { "wfx-category-sound" } else { "wfx-category" };
    let entry = list.read().clone();
    let s_add = t("lblFxAdd");
    let s_all = t("optFxCategoryAll");
    let s_remove = t("tipFxRemove");

    // One entry per category, in the catalogue's order; inside it the standard effects (in their fixed order), then the added ones.
    let (heads, cat_of) = category_index(sound);
    let mut groups: Vec<Group> = heads.into_iter().map(|(id, title)| Group { id, title, on: 0, rows: Vec::new(), extras: Vec::new() }).collect();
    for key in rows_std.iter().filter(|key| !widgets.contains(key)) {
        let found = entry.items.iter().cloned().enumerate().find(|(_, i)| &i.key == key);
        let g = group_of(&mut groups, cat_of.get(key).map(String::as_str).unwrap_or_default());
        g.on += usize::from(found.is_some());
        g.rows.push((key.clone(), found));
    }
    for (at, item) in entry.items.iter().cloned().enumerate().filter(|(_, i)| !standard.contains(&i.key) && effects::is_sound(&i.key) == sound) {
        let g = group_of(&mut groups, cat_of.get(&item.key).map(String::as_str).unwrap_or_default());
        g.on += 1;
        g.extras.push((at, item));
    }
    groups.retain(|g| !g.rows.is_empty() || !g.extras.is_empty());
    let cat_pick = cat_of.clone();

    rsx! {
        div { class: "wfx",
            for g in groups {
              {
                let cid = format!("cat:{}", g.id);
                let group_shown = open.read().contains(&cid);
                let cid_toggle = cid.clone();
                rsx! {
                  div { class: "wfx-cat", key: "{cid}",
                    div { class: "fx-head",
                        button {
                            class: "fx-title",
                            r#type: "button",
                            "aria-expanded": "{group_shown}",
                            onclick: move |_| {
                                let mut o = open.write();
                                if !o.remove(&cid_toggle) {
                                    o.insert(cid_toggle.clone());
                                }
                            },
                            if group_shown {
                                Icon { width: 12, height: 12, icon: LdChevronDown }
                            } else {
                                Icon { width: 12, height: 12, icon: LdChevronRight }
                            }
                            span { class: "wfx-title", "{g.title}" }
                            if g.on > 0 {
                                span { class: "wfx-mixed", "{g.on}" }
                            }
                        }
                    }
                    if group_shown {
                      div { class: "wfx-group",
            for (key, found) in g.rows {
                {
                    let item = found.as_ref().map(|(_, i)| i.clone()).unwrap_or_else(|| Item::new(&key));
                    let on = found.is_some();
                    let shown = on && open.read().contains(&key);
                    let (key_switch, key_toggle, std_switch) = (key.clone(), key.clone(), standard.clone());
                    let cid_on = cid.clone();
                    rsx! {
                        Row {
                            key: "{key}",
                            title: item.title(),
                            remove: s_remove.clone(),
                            switch: Some(on),
                            partial: mixed.partial.contains(&key),
                            differs: mixed.differing.contains(&key),
                            open: shown,
                            on_switch: move |now: bool| {
                                let k = key_switch.clone();
                                edit(list, &std_switch, |e| {
                                    if now {
                                        e.items.push(Item::new(&k));
                                    } else {
                                        e.items.retain(|i| i.key != k);
                                    }
                                });
                                // switched on: its form opens; switched off: it closes
                                let mut o = open.write();
                                if now {
                                    o.insert(k);
                                    o.insert(cid_on.clone());
                                } else {
                                    o.remove(&k);
                                }
                            },
                            on_toggle: move |_| {
                                let mut o = open.write();
                                if !o.remove(&key_toggle) {
                                    o.insert(key_toggle.clone());
                                }
                            },
                            on_remove: move |_| {},
                            if let Some((at, item)) = found {
                                Form { list, lang: lang.clone(), at, item, mixed: mixed.controls.clone(), touched }
                            }
                        }
                    }
                }
            }
            for (at, item) in g.extras {
                {
                    let id = format!("x{at}");
                    let shown = open.read().contains(&id);
                    let (id_toggle, std_remove) = (id.clone(), standard.clone());
                    rsx! {
                        Row {
                            key: "{id}-{item.key}",
                            title: item.title(),
                            remove: s_remove.clone(),
                            switch: None,
                            open: shown,
                            on_switch: move |_| {},
                            on_toggle: move |_| {
                                let mut o = open.write();
                                if !o.remove(&id_toggle) {
                                    o.insert(id_toggle.clone());
                                }
                            },
                            on_remove: move |_| {
                                edit(list, &std_remove, |e| {
                                    if at < e.items.len() {
                                        e.items.remove(at);
                                    }
                                });
                                // the places after it moved: every open added card closes
                                open.write().retain(|k| !k.starts_with('x'));
                            },
                            Form { list, lang: lang.clone(), at, item, mixed: mixed.controls.clone(), touched }
                        }
                    }
                }
            }
                      }
                    }
                  }
                }
              }
            }
            if effects::catalog().is_none() {
                p { class: "uis-hnt", {t("msgFxNoCatalogue")} }
            }
            if !sound {
                for problem in entry.order_problems() {
                    p { class: "uis-hnt", key: "{problem}", "{problem}" }
                }
            }
            // the add row: a category filter welded to the search box, one catalog control
            div { class: "wfx-add",
                    {
                        let rows = work_menu_rows(sound, &query(), &category());
                        let items: Vec<UisComboItem> = rows.iter().map(|(k, l, header)| UisComboItem { key: k.clone(), label: l.clone(), active: false, header: *header }).collect();
                        let std_add = standard.clone();
                        rsx! {
                            UisCboFilter {
                                id: wfx_category,
                                icon: rsx! { Icon { width: 14, height: 14, icon: LdListFilter } },
                                all_label: s_all.clone(),
                                categories: menu_of(sound).into_iter().map(|(id, title, _)| UisCboCategory { id, title, icon: None }).collect::<Vec<_>>(),
                                category: category(),
                                on_category: move |c: String| category.set(c),
                                value: query(),
                                placeholder: s_add.clone(),
                                items,
                                on_change: move |v: String| query.set(v),
                                on_pick: move |i: usize| {
                                    let Some((key, _, false)) = work_menu_rows(sound, &query(), &category()).get(i).cloned() else { return };
                                    // a standard effect is already a row: picking it switches it on and opens it
                                    // its category opens too, so the effect is seen where it landed
                                    let group = format!("cat:{}", cat_pick.get(&key).cloned().unwrap_or_default());
                                    if std_add.contains(&key) {
                                        if !list.peek().items.iter().any(|x| x.key == key) {
                                            edit(list, &std_add, |e| e.items.push(Item::new(&key)));
                                        }
                                        open.write().insert(key);
                                    } else {
                                        let place = list.peek().items.len();
                                        edit(list, &std_add, |e| e.items.push(Item::new(&key)));
                                        open.write().insert(format!("x{place}"));
                                    }
                                    open.write().insert(group);
                                    query.set(String::new());
                                },
                            }
                        }
                    }
            }
        }
    }
}
