//! Turns off ONNX Runtime's CPU memory arena and intra-op spin-waiting for every session sherpa-onnx creates.
//!
//! The arena grows to fit the largest tensors ever seen and never shrinks: measured +198 MB on the
//! Parakeet encoder after 4-12 s decodes, vs +8 MB without it, for ~5% slower encodes. sherpa-onnx
//! has no switch for it (k2-fsa/sherpa-onnx#3032), so `build.rs` links with
//! `--wrap=OrtGetApiBase`: sherpa's lookup of the ORT C API lands here, and we hand it a copy of the
//! API table whose CreateSession entries call ORT's own `DisableCpuMemArena` on the options first.
//!
//! Tied to the bundled ORT 1.28.2 (`sherpa-onnx-sys =1.13.8`). The OrtApi table is append-only ABI,
//! so the indices below are stable; API_LEN must match the bundled build (checked by `nm -S`:
//! `ort_api_1_to_28` is 0xd40 bytes).

use std::ffi::{c_char, c_void};
use std::sync::OnceLock;

const API_VERSION: u32 = 28;
const API_LEN: usize = 0xd40 / 8;
const CREATE_SESSION: usize = 7;
const CREATE_SESSION_FROM_ARRAY: usize = 8;
const DISABLE_CPU_MEM_ARENA: usize = 19;
const RELEASE_STATUS: usize = 93;
const ADD_SESSION_CONFIG_ENTRY: usize = 130;

type Status = *mut c_void;
type CreateSession = unsafe extern "C" fn(*const c_void, *const c_char, *const c_void, *mut *mut c_void) -> Status;
type CreateSessionFromArray =
    unsafe extern "C" fn(*const c_void, *const c_void, usize, *const c_void, *mut *mut c_void) -> Status;
type AddConfig = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> Status;
type DisableArena = unsafe extern "C" fn(*mut c_void) -> Status;
type ReleaseStatus = unsafe extern "C" fn(Status);

#[repr(C)]
pub struct OrtApiBase {
    get_api: unsafe extern "C" fn(u32) -> *const c_void,
    get_version_string: unsafe extern "C" fn() -> *const c_char,
}

extern "C" {
    fn __real_OrtGetApiBase() -> *const OrtApiBase;
}

static BASE: OrtApiBase = OrtApiBase { get_api, get_version_string };
/// The real table and our patched copy, as raw entries (function addresses).
static TABLES: OnceLock<(usize, Box<[usize; API_LEN]>)> = OnceLock::new();

fn tables() -> &'static (usize, Box<[usize; API_LEN]>) {
    TABLES.get_or_init(|| unsafe {
        let real = ((*__real_OrtGetApiBase()).get_api)(API_VERSION) as *const [usize; API_LEN];
        assert!(!real.is_null(), "bundled ONNX Runtime does not provide API version {API_VERSION}");
        let mut patched = Box::new(*real);
        patched[CREATE_SESSION] = create_session as CreateSession as usize;
        patched[CREATE_SESSION_FROM_ARRAY] = create_session_from_array as CreateSessionFromArray as usize;
        (real as usize, patched)
    })
}

unsafe fn real<T: Copy>(index: usize) -> T {
    let entry = (*(tables().0 as *const [usize; API_LEN]))[index];
    std::mem::transmute_copy(&entry)
}

unsafe fn disable_arena(options: *const c_void) {
    let status = real::<DisableArena>(DISABLE_CPU_MEM_ARENA)(options as *mut c_void);
    if !status.is_null() {
        real::<ReleaseStatus>(RELEASE_STATUS)(status); // keep the arena rather than fail the load
    }
    // Idle intra-op pool threads spin-wait by default and burn a core each between ops.
    let status = real::<AddConfig>(ADD_SESSION_CONFIG_ENTRY)(
        options as *mut c_void,
        c"session.intra_op.allow_spinning".as_ptr(),
        c"0".as_ptr(), // measured: spinning on = 12x the CPU seconds
    );
    if !status.is_null() {
        real::<ReleaseStatus>(RELEASE_STATUS)(status);
    }
}

unsafe extern "C" fn create_session(
    env: *const c_void,
    path: *const c_char,
    options: *const c_void,
    out: *mut *mut c_void,
) -> Status {
    disable_arena(options);
    real::<CreateSession>(CREATE_SESSION)(env, path, options, out)
}

unsafe extern "C" fn create_session_from_array(
    env: *const c_void,
    data: *const c_void,
    len: usize,
    options: *const c_void,
    out: *mut *mut c_void,
) -> Status {
    disable_arena(options);
    real::<CreateSessionFromArray>(CREATE_SESSION_FROM_ARRAY)(env, data, len, options, out)
}

unsafe extern "C" fn get_api(version: u32) -> *const c_void {
    if version > API_VERSION {
        return ((*__real_OrtGetApiBase()).get_api)(version); // newer than we patch: untouched
    }
    tables().1.as_ptr() as *const c_void
}

unsafe extern "C" fn get_version_string() -> *const c_char {
    ((*__real_OrtGetApiBase()).get_version_string)()
}

#[no_mangle]
pub extern "C" fn __wrap_OrtGetApiBase() -> *const OrtApiBase {
    &BASE
}
