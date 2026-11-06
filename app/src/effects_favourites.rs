//! The favourites, a part of the ffmpeg page of Settings: every effect the app can offer, under the headings of its categories, with a box to
//! tick the ones the work view lists (ready to switch on). The catalogue's standard effects are ticked until the user chooses; the choice is
//! `favourites.json`, beside `effects.json`. The same file in the FFmpeg Video Studio and the FFmpeg Audio Studio: what an app is (`effects::WORKS`)
//! is the only thing it says.
//!
//! ⚠ SOURCE OF TRUTH: the shared ffmpeg-effects folder (`rust/app`). A copy in an app is never edited there.
//! It needs, from the app: `crate::i18n`, `crate::effects_editor::{menu_all, SYSTEM}`, and `engine::effects` (`catalog`, `favourites`,
//! `set_favourites`, `is_locked`, `is_standard`, `WORKS`).

use crate::{
    effects_editor::{menu_all, SYSTEM},
    i18n,
};
use dioxus::prelude::*;
use engine::effects;

/// Every effect the app can list: the ones of its own kind and the ones that act on the sound alone (in an app that converts sound, the same).
fn offered<E>(own: Vec<E>, sound: Vec<E>, key: impl Fn(&E) -> &str) -> Vec<E> {
    let mut entries = own;
    for e in sound {
        if !entries.iter().any(|o| key(o) == key(&e)) {
            entries.push(e);
        }
    }
    entries
}

/// The keys the work view lists now, locked or not.
fn listed() -> Vec<String> {
    effects::catalog()
        .map(|c| offered(c.picker(SYSTEM, effects::WORKS), c.picker_for(SYSTEM, "audio", true), |e| &e.key).into_iter().filter(effects::is_standard).map(|e| e.key).collect())
        .unwrap_or_default()
}

#[component]
pub fn EffectsFavourites(lang: String) -> Element {
    let mut query = use_signal(String::new);
    // What the user chose, read again after every change so the boxes follow the file.
    let mut chosen = use_signal(effects::favourites);
    let mut failure = use_signal(|| Option::<String>::None);
    let t = |key: &str| i18n::t(&lang, key);

    let Some(cat) = effects::catalog() else {
        return rsx! {
            div { class: "uis-fld-labelled divider",
                label { class: "uis-fld-labelled-label", {t("lblFxFavourites")} }
                p { class: "uis-hnt", {t("msgFxNoCatalogue")} }
            }
        };
    };
    let _ = chosen();
    let entries = offered(cat.picker(SYSTEM, effects::WORKS), cat.picker_for(SYSTEM, "audio", true), |e| &e.key);
    let locked: Vec<String> = entries.iter().filter(|e| effects::is_locked(e)).map(|e| e.key.clone()).collect();
    let on = listed();
    let q = query().trim().to_lowercase();
    let groups: Vec<_> = menu_all()
        .into_iter()
        .map(|(id, heading, items)| {
            let all = q.is_empty() || heading.to_lowercase().contains(&q);
            let rows: Vec<_> = items.into_iter().filter(|(_, title, what)| all || title.to_lowercase().contains(&q) || what.to_lowercase().contains(&q)).collect();
            (id, heading, rows)
        })
        .filter(|(_, _, rows)| !rows.is_empty())
        .collect();
    let count = t("msgFxFavouritesCount").replace("{n}", &on.len().to_string()).replace("{total}", &entries.len().to_string());
    let (tip_locked, search) = (t("tipFxLocked"), t("phFxFavouritesSearch"));

    rsx! {
        div { class: "uis-fld-labelled divider",
            label { class: "uis-fld-labelled-label", {t("lblFxFavourites")} }
            p { class: "uis-hnt", {t("msgFxFavouritesHint")} }
            div { class: "fav-bar",
                input {
                    class: "uis-txt",
                    r#type: "text",
                    value: "{query}",
                    placeholder: "{search}",
                    oninput: move |ev| query.set(ev.value()),
                }
                span { class: "uis-hnt", "{count}" }
                button {
                    class: "uis-btn",
                    r#type: "button",
                    disabled: chosen().is_none(),
                    onclick: move |_| match effects::set_favourites(None) {
                        Ok(()) => {
                            failure.set(None);
                            chosen.set(effects::favourites());
                        }
                        Err(e) => failure.set(Some(e)),
                    },
                    {t("btnFxFavouritesReset")}
                }
            }
            if let Some(reason) = failure() {
                p { class: "uis-hnt", {t("msgFxFavouritesFailed").replace("{reason}", &reason)} }
            }
            for (id, heading, rows) in groups {
                div { class: "fav-group", key: "{id}",
                    div { class: "fav-head", "{heading}" }
                    for (key, title, what) in rows {
                        {
                            let is_locked = locked.contains(&key);
                            let checked = on.contains(&key);
                            let (listed_now, flip) = (on.clone(), key.clone());
                            let locked_now = locked.clone();
                            rsx! {
                                label {
                                    class: if is_locked { "fav-row locked" } else { "fav-row" },
                                    key: "{key}",
                                    title: if is_locked { tip_locked.clone() } else { what.clone() },
                                    input {
                                        r#type: "checkbox",
                                        checked,
                                        disabled: is_locked,
                                        onchange: move |ev| {
                                            let mut keys: Vec<String> = listed_now.iter().filter(|k| !locked_now.contains(k)).cloned().collect();
                                            keys.retain(|k| *k != flip);
                                            if ev.checked() {
                                                keys.push(flip.clone());
                                            }
                                            match effects::set_favourites(Some(keys)) {
                                                Ok(()) => {
                                                    failure.set(None);
                                                    chosen.set(effects::favourites());
                                                }
                                                Err(e) => failure.set(Some(e)),
                                            }
                                        },
                                    }
                                    span { class: "fav-title", "{title}" }
                                    span { class: "fav-what", "{what}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
