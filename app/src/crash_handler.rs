//! A native crash report for Windows.
//!
//! A Rust panic is recorded in the log by the panic hook, but a crash inside
//! a native library (projectM, ANGLE, the GPU driver) kills the process
//! without any Rust code running, so the log stays empty. This installs an
//! unhandled exception filter that writes what Windows knows at that point:
//! the exception, the module and offset where it happened and the call
//! stack. It then lets Windows carry on as usual.
//!
//! The filter appends to the log file directly instead of going through
//! `error_log`: the crash may have happened while that log's lock was held.

/// Name of an exception code, for the report.
#[cfg(any(windows, test))]
fn exception_name(code: u32) -> &'static str {
    match code {
        0xC000_0005 => "access violation",
        0xC000_001D => "illegal instruction",
        0xC000_0094 => "integer divide by zero",
        0xC000_00FD => "stack overflow",
        0xC000_0374 => "heap corruption",
        0xC000_0409 => "fast fail (stack buffer overrun)",
        0xE06D_7363 => "C++ exception",
        _ => "unknown exception",
    }
}

/// `module+0xoffset` for an address, or the bare address when no module owns it.
#[cfg(any(windows, test))]
fn format_location(address: usize, module: Option<(&str, usize)>) -> String {
    match module {
        Some((name, base)) if address >= base => format!("{name}+{:#x}", address - base),
        _ => format!("{address:#x}"),
    }
}

/// The report: one line for the exception, then one per stack frame.
#[cfg(any(windows, test))]
fn format_report(code: u32, address: usize, frames: &[String]) -> String {
    let mut report = format!("[crash] {} ({code:#010x}) at {}", exception_name(code), frames.first().map_or_else(|| format!("{address:#x}"), Clone::clone));
    for frame in frames {
        report.push_str("\n[crash]   ");
        report.push_str(frame);
    }
    report
}

#[cfg(windows)]
pub(crate) use imp::install;

#[cfg(not(windows))]
pub(crate) fn install(_log_path: std::path::PathBuf) {}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    use super::{format_location, format_report};

    #[repr(C)]
    pub(super) struct ExceptionRecord {
        pub(super) code: u32,
        pub(super) flags: u32,
        pub(super) record: *mut ExceptionRecord,
        pub(super) address: *mut c_void,
        pub(super) number_parameters: u32,
        pub(super) information: [usize; 15],
    }

    #[repr(C)]
    pub(super) struct ExceptionPointers {
        pub(super) record: *mut ExceptionRecord,
        pub(super) context: *mut c_void,
    }

    type Filter = unsafe extern "system" fn(*mut ExceptionPointers) -> i32;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetUnhandledExceptionFilter(filter: Option<Filter>) -> Option<Filter>;
        fn RtlCaptureStackBackTrace(skip: u32, count: u32, frames: *mut *mut c_void, hash: *mut u32) -> u16;
        fn GetModuleHandleExW(flags: u32, address: *const c_void, module: *mut *mut c_void) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, buffer: *mut u16, size: u32) -> u32;
        fn GetCurrentProcess() -> *mut c_void;
        fn ReadProcessMemory(process: *mut c_void, address: *const c_void, buffer: *mut c_void, size: usize, read: *mut usize) -> i32;
    }

    /// Offsets of the general-purpose registers in the x64 `CONTEXT` record.
    const REGISTERS: [(&str, usize); 16] = [
        ("rax", 0x78),
        ("rcx", 0x80),
        ("rdx", 0x88),
        ("rbx", 0x90),
        ("rsp", 0x98),
        ("rbp", 0xA0),
        ("rsi", 0xA8),
        ("rdi", 0xB0),
        ("r8", 0xB8),
        ("r9", 0xC0),
        ("r10", 0xC8),
        ("r11", 0xD0),
        ("r12", 0xD8),
        ("r13", 0xE0),
        ("r14", 0xE8),
        ("r15", 0xF0),
    ];

    /// Reads `buffer.len()` bytes at `address` without faulting; `false` when unreadable.
    fn read_memory(address: usize, buffer: &mut [u8]) -> bool {
        let mut read = 0usize;
        let ok = unsafe { ReadProcessMemory(GetCurrentProcess(), address as *const c_void, buffer.as_mut_ptr().cast(), buffer.len(), &mut read) };
        ok != 0 && read == buffer.len()
    }

    /// The registers, and 32 bytes behind each one that points to readable memory.
    unsafe fn describe_registers(context: *mut c_void) -> Vec<String> {
        if context.is_null() {
            return Vec::new();
        }
        let mut lines = Vec::new();
        for (name, offset) in REGISTERS {
            let value = *(context.cast::<u8>().add(offset).cast::<usize>());
            let mut line = format!("{name}={value:#x}");
            let mut bytes = [0u8; 32];
            if value > 0x10000 && read_memory(value, &mut bytes) {
                line.push_str(" ->");
                for chunk in bytes.chunks(8) {
                    line.push_str(&format!(" {:016x}", u64::from_le_bytes(chunk.try_into().unwrap())));
                }
            }
            lines.push(line);
        }
        lines
    }

    const FROM_ADDRESS: u32 = 0x4;
    const UNCHANGED_REFCOUNT: u32 = 0x2;
    const CONTINUE_SEARCH: i32 = 0;
    const MAX_FRAMES: usize = 32;

    static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

    pub(crate) fn install(log_path: PathBuf) {
        let _ = LOG_PATH.set(log_path);
        unsafe { SetUnhandledExceptionFilter(Some(filter)) };
    }

    /// `module+offset` of the code at `address`.
    fn locate(address: *mut c_void) -> String {
        let mut module: *mut c_void = std::ptr::null_mut();
        let found = unsafe { GetModuleHandleExW(FROM_ADDRESS | UNCHANGED_REFCOUNT, address, &mut module) } != 0;
        if !found || module.is_null() {
            return format_location(address as usize, None);
        }
        let mut buffer = [0u16; 260];
        let length = unsafe { GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
        let path = String::from_utf16_lossy(&buffer[..length.min(buffer.len())]);
        let name = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string();
        format_location(address as usize, Some((&name, module as usize)))
    }

    pub(super) unsafe extern "system" fn filter(info: *mut ExceptionPointers) -> i32 {
        if info.is_null() || (*info).record.is_null() {
            return CONTINUE_SEARCH;
        }
        let record = &*(*info).record;
        let mut stack = [std::ptr::null_mut::<c_void>(); MAX_FRAMES];
        let captured = RtlCaptureStackBackTrace(0, MAX_FRAMES as u32, stack.as_mut_ptr(), std::ptr::null_mut()) as usize;
        let mut frames = vec![locate(record.address)];
        frames.extend(stack[..captured].iter().map(|&address| locate(address)));
        let mut report = format_report(record.code, record.address as usize, &frames);
        for line in describe_registers((*info).context) {
            report.push_str("\n[crash]   ");
            report.push_str(&line);
        }
        eprintln!("{report}");
        if let Some(path) = LOG_PATH.get() {
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{report}");
            }
        }
        CONTINUE_SEARCH
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_filter_writes_the_exception_module_and_offset_to_the_log() {
            let dir = std::env::temp_dir().join(format!("opendrop-crash-test-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("opendrop.log");
            install(path.clone());

            let mut record = ExceptionRecord {
                code: 0xC000_0005,
                flags: 0,
                record: std::ptr::null_mut(),
                address: (filter as Filter) as usize as *mut c_void,
                number_parameters: 0,
                information: [0; 15],
            };
            let mut info = ExceptionPointers { record: &mut record, context: std::ptr::null_mut() };
            let result = unsafe { filter(&mut info) };

            assert_eq!(result, CONTINUE_SEARCH);
            let written = std::fs::read_to_string(LOG_PATH.get().unwrap()).unwrap();
            assert!(written.contains("[crash] access violation (0xc0000005) at "), "{written}");
            assert!(written.contains(".exe+0x"), "the faulting address should resolve to a module: {written}");
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exception_codes_have_readable_names() {
        assert_eq!(exception_name(0xC000_0005), "access violation");
        assert_eq!(exception_name(0xC000_00FD), "stack overflow");
        assert_eq!(exception_name(0x1234), "unknown exception");
    }

    #[test]
    fn a_location_is_the_module_and_offset_or_the_bare_address() {
        assert_eq!(format_location(0x7ff6_0000_1234, Some(("projectM-4.dll", 0x7ff6_0000_0000))), "projectM-4.dll+0x1234");
        assert_eq!(format_location(0x10, Some(("a.dll", 0x1000))), "0x10");
        assert_eq!(format_location(0xdead, None), "0xdead");
    }

    #[test]
    fn the_report_names_the_exception_and_lists_every_frame() {
        let frames = vec!["libGLESv2.dll+0x10".to_string(), "opendrop-app.exe+0x20".to_string()];
        let report = format_report(0xC000_0005, 0x1, &frames);
        assert_eq!(
            report,
            "[crash] access violation (0xc0000005) at libGLESv2.dll+0x10\n[crash]   libGLESv2.dll+0x10\n[crash]   opendrop-app.exe+0x20"
        );
    }
}
