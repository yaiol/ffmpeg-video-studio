// twin: uis-news-loader.jsx jsx@5fec0213e3f6 rsx@4218dd7b6749
//! UisNewsLoader — twin of `uis-news-loader.jsx`: the one-line wiring that makes an app show yaiol news.
//! Once on launch it asks the news module what this app should show; if there is anything it renders the
//! shared `UisDlgNews`, and when the reader closes it, remembers the ids they asked not to see again.
//! Silent on every failure. Always render it: the news module decides what is shown. A new version comes first
//! and is shown whatever the user’s "Show news from yaiol" setting says; with the setting off no announcement is
//! shown or requested. Pass `app_version` (the running release version) to have the new-version check; left empty
//! there is none, as for an app that has not adopted it.
//!
//!   UisNewsLoader { app_id: APP_ID, app_version: APP_VERSION, lang: lang() }
//!
//! TWO REMINDER BUTTONS for the window bar, the last children of `UisBarWindow` (after settings, help, source),
//! in this order. Each draws nothing until the loader's one launch request has found something for it, then
//! stays for the whole session - also after the dialog was closed or a card silenced with "don't show again",
//! which is their point: a way back. Neither makes a request of its own.
//!
//! UisBtnUpdate — a newer version is out. A click reopens that update card alone; its tooltip is the card's
//! title (`lblUpdateAvailable` + the version). No newer version known (up to date, offline, no launch request)
//! = no button.
//!
//! UisBtnNews — yaiol has news for this app: at least one announcement that is live today (started, not ended)
//! and targeted at it, silenced ones included. A click reopens the announcements; tooltip `tipNewsShow`. News
//! switched off (no announcement is requested) or every announcement ended = no button.
//!
//!   UisBarWindow { …, /* settings, help, source */ UisBtnUpdate { lang: lang() } UisBtnNews { lang: lang() } }
//!
//! A card reopened from a button shows its "don't show again" box as it stands (ticked once silenced);
//! unticking it brings the launch notice back.
//!
//! The JSX twin asks its app’s local server over HTTP; here the module is called directly, off the UI
//! thread. The dialog reads its four words from the app's own `crate::i18n`.
//! Differs: behaviour none. Form only: it calls the news module directly, where the JSX asks its app's local server
//! over HTTP; the buttons read their tooltip from the app's own `crate::i18n` (they take `lang`, the JSX a passed
//! `t`); what the loader and the buttons share is global signals, where the JSX keeps module variables.

use super::dlg_news::{UisDlgNews, UisNewsItem};
use super::news;
use crate::i18n;
use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::ld_icons::{LdDownload, LdNewspaper},
    Icon,
};

// What the launch request learned, kept for the session: the update card (or none) and the live announcements.
// Written by the loader, read by the two reminder buttons.
static KNOWN_UPDATE: GlobalSignal<Option<UisNewsItem>> = Signal::global(|| None);
static KNOWN_NEWS: GlobalSignal<Vec<UisNewsItem>> = Signal::global(Vec::new);
// The cards the dialog is showing now (none = no dialog). The loader sets it on launch, a button to reopen.
static SHOWN: GlobalSignal<Option<Vec<UisNewsItem>>> = Signal::global(|| None);

#[component]
pub fn UisNewsLoader(app_id: String, lang: String, #[props(default)] app_version: String) -> Element {
    // Once, on launch.
    {
        let (app_id, app_version, lang) = (app_id.clone(), app_version.clone(), lang.clone());
        use_future(move || {
            let (app_id, app_version, lang) = (app_id.clone(), app_version.clone(), lang.clone());
            async move {
                // The user’s own date, from the page: the standard library has no local time zone.
                let today = document::eval("const d = new Date(); dioxus.send(d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0'));")
                    .recv::<String>()
                    .await
                    .unwrap_or_default();
                let found = tokio::task::spawn_blocking(move || news::current(&app_id, &app_version, &lang, &today)).await.unwrap_or_default();
                *KNOWN_UPDATE.write() = found.iter().find(|it| it.kind == "update").cloned();
                *KNOWN_NEWS.write() = found.iter().filter(|it| it.kind != "update").cloned().collect();
                // A silenced card is not shown on launch: it is only kept, for its reminder button.
                let show: Vec<UisNewsItem> = found.into_iter().filter(|it| !it.dismissed).collect();
                if !show.is_empty() {
                    *SHOWN.write() = Some(show);
                }
            }
        });
    }

    let Some(list) = SHOWN() else { return rsx! {} };
    let shown = list.clone();
    rsx! {
        UisDlgNews {
            items: list,
            lang,
            on_close: move |hide_ids: Vec<String>| {
                // Every card that was shown: its box may have been ticked, or (reopened already silenced)
                // unticked, which brings its launch notice back.
                let restore: Vec<String> = shown.iter().filter(|it| it.dismissed && !hide_ids.contains(&it.id)).map(|it| it.id.clone()).collect();
                let flag = |it: &mut UisNewsItem| {
                    if shown.iter().any(|s| s.id == it.id) {
                        it.dismissed = hide_ids.contains(&it.id);
                    }
                };
                if let Some(update) = KNOWN_UPDATE.write().as_mut() {
                    flag(update);
                }
                KNOWN_NEWS.write().iter_mut().for_each(flag);
                if !hide_ids.is_empty() || !restore.is_empty() {
                    spawn(async move {
                        let _ = tokio::task::spawn_blocking(move || {
                            news::dismiss(&hide_ids);
                            news::restore(&restore);
                        })
                        .await;
                    });
                }
                *SHOWN.write() = None;
            },
        }
    }
}

#[component]
pub fn UisBtnUpdate(lang: String) -> Element {
    let Some(update) = KNOWN_UPDATE() else { return rsx! {} };
    let tip = format!("{} v{}", i18n::t(&lang, "lblUpdateAvailable"), update.version);
    rsx! {
        button {
            class: "uis-barh-window-btn update",
            title: "{tip}",
            "aria-label": "{tip}",
            onclick: move |_| *SHOWN.write() = Some(vec![update.clone()]),
            Icon { width: 14, height: 14, icon: LdDownload }
        }
    }
}

#[component]
pub fn UisBtnNews(lang: String) -> Element {
    if KNOWN_NEWS.read().is_empty() {
        return rsx! {};
    }
    let tip = i18n::t(&lang, "tipNewsShow");
    rsx! {
        button {
            class: "uis-barh-window-btn",
            title: "{tip}",
            "aria-label": "{tip}",
            onclick: move |_| {
                let cards = KNOWN_NEWS();
                *SHOWN.write() = Some(cards);
            },
            Icon { width: 14, height: 14, icon: LdNewspaper }
        }
    }
}
