use windows::core::{s, w};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

const ACCENT_ENABLE_BLURBEHIND: i32 = 3;
const ACCENT_ENABLE_ACRYLICBLURBEHIND: i32 = 4;
const ACCENT_FLAG_DRAW_ALL: i32 = 2;
const WCA_ACCENT_POLICY: i32 = 19;

#[repr(C)]
struct AccentPolicy {
    accent_state: i32,
    accent_flags: i32,
    gradient_color: u32,
    animation_id: i32,
}

#[repr(C)]
struct WindowCompositionAttributeData {
    attribute: i32,
    data: *mut core::ffi::c_void,
    size_of_data: usize,
}

type SetWindowCompositionAttributeFn =
    unsafe extern "system" fn(HWND, *mut WindowCompositionAttributeData) -> i32;

unsafe fn set_window_composition_attribute() -> Option<SetWindowCompositionAttributeFn> {
    let user32 = GetModuleHandleW(w!("user32.dll")).ok()?;
    let proc = GetProcAddress(user32, s!("SetWindowCompositionAttribute"))?;
    Some(std::mem::transmute::<
        unsafe extern "system" fn() -> isize,
        SetWindowCompositionAttributeFn,
    >(proc))
}

#[allow(unused_assignments)]
pub fn apply(hwnd: HWND, light: bool) {
    // AABBGGRR. Keep the tint very low so the CSS glass can do the colour work.
    let gradient = if light {
        0x14F4F4F4u32
    } else {
        0x180C0C0Eu32
    };

    unsafe {
        let Some(setter) = set_window_composition_attribute() else {
            return;
        };

        let mut policy = AccentPolicy {
            accent_state: ACCENT_ENABLE_ACRYLICBLURBEHIND,
            accent_flags: ACCENT_FLAG_DRAW_ALL,
            gradient_color: gradient,
            animation_id: 0,
        };

        let mut data = WindowCompositionAttributeData {
            attribute: WCA_ACCENT_POLICY,
            data: &mut policy as *mut AccentPolicy as *mut core::ffi::c_void,
            size_of_data: std::mem::size_of::<AccentPolicy>(),
        };

        if setter(hwnd, &mut data) == 0 {
            policy.accent_state = ACCENT_ENABLE_BLURBEHIND;
            let _ = setter(hwnd, &mut data);
        }
    }
}

