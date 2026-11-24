//! What the machine is doing: memory in use on the graphics cards (performance counters, the same ones
//! the Task Manager shows) and whether it is low on power.

use std::ptr;

use ep_os::Machine;
use windows_sys::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_LARGE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW,
};
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

/// Less charge than this (percent) is low.
const LOW: u8 = 50;

/// Windows' own numbers.
#[derive(Debug, Default)]
pub struct WinMachine;

/// A query that is closed when it goes.
struct Query(PDH_HQUERY);
impl Drop for Query {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: the handle came from `PdhOpenQueryW` and is closed once.
        unsafe { PdhCloseQuery(self.0) };
    }
}

impl Machine for WinMachine {
    #[allow(unsafe_code)]
    fn graphics_used(&self) -> Option<u64> {
        let path = super::wide(std::ffi::OsStr::new(
            r"\GPU Adapter Memory(*)\Dedicated Usage",
        ));
        let mut q: PDH_HQUERY = ptr::null_mut();
        // SAFETY: the out pointer is valid for the call.
        if unsafe { PdhOpenQueryW(ptr::null(), 0, &raw mut q) } != 0 {
            return None;
        }
        let q = Query(q);
        let mut counter: PDH_HCOUNTER = ptr::null_mut();
        // SAFETY: the query is open, the path ends with NUL, the out pointer is valid.
        if unsafe { PdhAddEnglishCounterW(q.0, path.as_ptr(), 0, &raw mut counter) } != 0 {
            return None;
        }
        // SAFETY: the query is open.
        if unsafe { PdhCollectQueryData(q.0) } != 0 {
            return None;
        }
        let (mut bytes, mut count) = (0u32, 0u32);
        // SAFETY: asks for the size only; the out pointers are valid.
        let r = unsafe {
            PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_LARGE,
                &raw mut bytes,
                &raw mut count,
                ptr::null_mut(),
            )
        };
        if r != PDH_MORE_DATA || bytes == 0 || bytes > 1 << 20 {
            return None;
        }
        // Whole items, so that the buffer is aligned for them.
        let size = size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        let mut items: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> = Vec::new();
        items.resize_with((bytes as usize).div_ceil(size), Default::default);
        // SAFETY: the buffer holds at least `bytes` bytes and is aligned; the out pointers are valid.
        let r = unsafe {
            PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_LARGE,
                &raw mut bytes,
                &raw mut count,
                items.as_mut_ptr(),
            )
        };
        if r != 0 {
            return None;
        }
        items
            .iter()
            .take(count as usize)
            .filter_map(|i| {
                // SAFETY: the values were formatted with `PDH_FMT_LARGE`, which fills this field.
                let v = unsafe { i.FmtValue.Anonymous.largeValue };
                u64::try_from(v).ok()
            })
            .max()
    }

    #[allow(unsafe_code)]
    fn low_on_power(&self) -> Option<bool> {
        let mut s = SYSTEM_POWER_STATUS::default();
        // SAFETY: the out pointer is valid for the call.
        if unsafe { GetSystemPowerStatus(&raw mut s) } == 0 {
            return None;
        }
        match s.ACLineStatus {
            // 255: unknown.
            0 => Some(s.BatteryLifePercent < LOW),
            1 => Some(false),
            _ => None,
        }
    }
}
