use std::collections::HashSet;
use std::sync::mpsc;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationSelectionItemPattern,
    TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_SelectionItemPatternId,
    UIA_TabItemControlTypeId,
};
use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;

use super::util::normalize_title;

const UIA_TIMEOUT: Duration = Duration::from_millis(3000);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabSelection {
    pub hwnd: i64,
    pub media_title: String,
    pub previous_title: String,
    pub switched: bool,
}

/// Selects the browser tab that matches the media title and remembers the
/// window title of the tab that was active before. Used by the live preview,
/// because a YouTube tab keeps playing while another tab owns the window title.
pub fn select_tab(hwnds: &[i64], title: &str) -> Option<TabSelection> {
    let wanted = normalize_title(title);
    if wanted.is_empty() || hwnds.is_empty() {
        return None;
    }

    let hwnds = hwnds.to_vec();
    let title = title.to_string();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let found = unsafe { select_inner(&hwnds, &title, &wanted) };
        let _ = sender.send(found);
    });

    receiver.recv_timeout(UIA_TIMEOUT).ok().flatten()
}

pub fn restore_tab(selection: &TabSelection) -> bool {
    if !selection.switched {
        return false;
    }

    let selection = selection.clone();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let restored = unsafe { restore_inner(&selection) };
        let _ = sender.send(restored);
    });

    receiver.recv_timeout(UIA_TIMEOUT).ok().unwrap_or(false)
}

unsafe fn automation() -> Option<IUIAutomation> {
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL).ok()
}

unsafe fn tab_condition(
    automation: &IUIAutomation,
) -> Option<windows::Win32::UI::Accessibility::IUIAutomationCondition> {
    let mut variant = VARIANT::default();
    let inner = &mut *variant.Anonymous.Anonymous;
    inner.vt = VT_I4;
    inner.Anonymous.lVal = UIA_TabItemControlTypeId.0;

    automation
        .CreatePropertyCondition(UIA_ControlTypePropertyId, &variant)
        .ok()
}

fn window_title(hwnd: i64) -> String {
    let mut buffer = [0u16; 512];
    let len = unsafe { GetWindowTextW(HWND(hwnd as *mut _), &mut buffer) };
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}

unsafe fn select_inner(hwnds: &[i64], title: &str, wanted: &str) -> Option<TabSelection> {
    let automation = automation()?;
    let condition = tab_condition(&automation)?;

    for hwnd in hwnds {
        let element = match automation.ElementFromHandle(HWND(*hwnd as *mut _)) {
            Ok(element) => element,
            Err(_) => continue,
        };
        let tabs = match element.FindAll(TreeScope_Descendants, &condition) {
            Ok(tabs) => tabs,
            Err(_) => continue,
        };

        let mut media: Option<IUIAutomationElement> = None;
        let count = tabs.Length().unwrap_or(0);
        for index in 0..count {
            let Ok(tab) = tabs.GetElement(index) else {
                continue;
            };
            let Ok(name) = tab.CurrentName() else {
                continue;
            };
            if tab_matches(&normalize_title(&name.to_string()), wanted) {
                media = Some(tab);
                break;
            }
        }

        let Some(media) = media else {
            continue;
        };

        let previous_title = window_title(*hwnd);
        if tab_matches(&normalize_title(&previous_title), wanted) {
            return Some(TabSelection {
                hwnd: *hwnd,
                media_title: title.to_string(),
                previous_title,
                switched: false,
            });
        }

        let pattern = media
            .GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
            .ok()?;
        pattern.Select().ok()?;

        return Some(TabSelection {
            hwnd: *hwnd,
            media_title: title.to_string(),
            previous_title,
            switched: true,
        });
    }

    None
}

unsafe fn restore_inner(selection: &TabSelection) -> bool {
    let current_title = window_title(selection.hwnd);
    if !tab_matches(
        &normalize_title(&current_title),
        &normalize_title(&selection.media_title),
    ) {
        // The user changed tabs while the preview was open: keep their choice.
        return false;
    }

    let Some(automation) = automation() else {
        return false;
    };
    let Some(condition) = tab_condition(&automation) else {
        return false;
    };
    let Ok(element) = automation.ElementFromHandle(HWND(selection.hwnd as *mut _)) else {
        return false;
    };
    let Ok(tabs) = element.FindAll(TreeScope_Descendants, &condition) else {
        return false;
    };

    let wanted = normalize_title(&strip_browser_suffix(&selection.previous_title));
    let mut best: Option<(IUIAutomationElement, usize)> = None;
    let count = tabs.Length().unwrap_or(0);

    for index in 0..count {
        let Ok(tab) = tabs.GetElement(index) else {
            continue;
        };
        let Ok(name) = tab.CurrentName() else {
            continue;
        };
        let title = stable_tab_title(&name.to_string());
        if title.is_empty() {
            continue;
        }

        let score = if title == wanted {
            10
        } else if title.contains(&wanted) || wanted.contains(&title) {
            8
        } else {
            let overlap = word_overlap(&title, &wanted);
            if overlap >= 4 && overlap * 2 >= wanted.split_whitespace().count() {
                overlap
            } else {
                0
            }
        };

        if score > 0 && best.as_ref().map(|(_, current)| score > *current).unwrap_or(true) {
            best = Some((tab, score));
        }
    }

    let Some((tab, _)) = best else {
        return false;
    };
    let Ok(pattern) =
        tab.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
    else {
        return false;
    };

    pattern.Select().is_ok()
}

fn stable_tab_title(value: &str) -> String {
    let normalized = normalize_title(value);
    let mut text = normalized.as_str();

    for prefix in ["uso elevado da memoria em ", "uso da memoria em "] {
        if let Some(rest) = text.strip_prefix(prefix) {
            text = rest;
        }
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() >= 2 {
        let amount = words[words.len() - 2];
        if words[words.len() - 1] == "mb" && amount.chars().all(|c| c.is_ascii_digit()) {
            return words[..words.len() - 2].join(" ");
        }
    }

    text.to_string()
}

fn strip_browser_suffix(value: &str) -> String {
    let lower = value.to_lowercase();
    for suffix in [
        " - google chrome",
        " - microsoft edge",
        " - mozilla firefox",
        " - brave",
        " - opera",
    ] {
        if let Some(index) = lower.rfind(suffix) {
            return value[..index].to_string();
        }
    }

    value.to_string()
}

fn word_overlap(a: &str, b: &str) -> usize {
    let words: HashSet<&str> = b.split_whitespace().filter(|word| word.len() > 2).collect();
    a.split_whitespace()
        .filter(|word| word.len() > 2 && words.contains(word))
        .count()
}

fn tab_matches(tab: &str, wanted: &str) -> bool {
    if tab.contains(wanted) || wanted.contains(tab) {
        return true;
    }

    let wanted_words: Vec<&str> = wanted.split_whitespace().collect();
    if wanted_words.len() < 4 {
        return false;
    }

    let tab_words: HashSet<&str> = tab.split_whitespace().collect();
    let common = wanted_words
        .iter()
        .filter(|word| word.len() > 2 && tab_words.contains(**word))
        .count();

    common >= 4 && common * 2 >= wanted_words.len()
}
