//! Wall-clock observation timestamps, not deadlines or elapsed-time accounting.
pub fn utc_timestamp() -> Option<String> {
    use windows::Win32::{
        Foundation::SYSTEMTIME,
        System::{SystemInformation::GetSystemTimeAsFileTime, Time::FileTimeToSystemTime},
    };
    let mut time = SYSTEMTIME::default();
    // SAFETY: the OS returns a value and writes only to the live SYSTEMTIME.
    unsafe {
        FileTimeToSystemTime(&GetSystemTimeAsFileTime(), &mut time).ok()?;
    }
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        time.wYear,
        time.wMonth,
        time.wDay,
        time.wHour,
        time.wMinute,
        time.wSecond,
        time.wMilliseconds
    ))
}
