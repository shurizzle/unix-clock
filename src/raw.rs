use cfg_if::cfg_if;
use core::{fmt, time::Duration};
use linux_syscalls::{syscall, Errno, Sysno};

const NSEC_PER_SEC: u64 = 1_000_000_000;
const I64_MAX: u64 = 9_223_372_036_854_775_807;

cfg_if! {
    if #[cfg(any(
        target_arch = "x86_64",
        target_arch = "powerpc64",
        target_arch = "mips64",
        target_arch = "s390x",
        target_arch = "sparc64"
    ))] {
        #[allow(non_upper_case_globals)]
        const SYS_clock_gettime: Sysno = Sysno::clock_gettime;
        #[allow(non_upper_case_globals)]
        const SYS_clock_settime: Sysno = Sysno::clock_settime;
    } else {
        #[allow(non_upper_case_globals)]
        const SYS_clock_gettime: Sysno = Sysno::clock_gettime64;
        #[allow(non_upper_case_globals)]
        const SYS_clock_settime: Sysno = Sysno::clock_settime64;
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClockId {
    /// A settable system-wide clock that measures real (i.e., wall-
    /// clock) time.  Setting this clock requires appropriate
    /// privileges.  This clock is affected by discontinuous jumps in
    /// the system time (e.g., if the system administrator manually
    /// changes the clock), and by the incremental adjustments performed
    /// by adjtime(3) and NTP.
    Realtime = 0,

    /// A nonsettable system-wide clock that represents monotonic time
    /// since—as described by POSIX—"some unspecified point in the
    /// past".  On Linux, that point corresponds to the number of
    /// seconds that the system has been running since it was booted.
    ///
    /// The CLOCK_MONOTONIC clock is not affected by discontinuous jumps
    /// in the system time (e.g., if the system administrator manually
    /// changes the clock), but is affected by the incremental
    /// adjustments performed by adjtime(3) and NTP.  This clock does
    /// not count time that the system is suspended.  All
    /// CLOCK_MONOTONIC variants guarantee that the time returned by
    /// consecutive calls will not go backwards, but successive calls
    /// may—depending on the architecture—return identical (not-
    /// increased) time values.
    Monotonic = 1,

    /// (since Linux 2.6.12)
    /// This is a clock that measures CPU time consumed by this process
    /// (i.e., CPU time consumed by all threads in the process).  On
    /// Linux, this clock is not settable.
    ProcessCputimeId = 2,

    /// (since Linux 2.6.12)
    /// This is a clock that measures CPU time consumed by this thread.
    /// On Linux, this clock is not settable.
    ThreadCputimeId = 3,

    /// (since Linux 2.6.28; Linux-specific)
    /// Similar to CLOCK_MONOTONIC, but provides access to a raw
    /// hardware-based time that is not subject to NTP adjustments or
    /// the incremental adjustments performed by adjtime(3).  This clock
    /// does not count time that the system is suspended.
    MonotonicRaw = 4,

    /// (since Linux 2.6.32; Linux-specific)
    /// A faster but less precise version of CLOCK_REALTIME.  This clock
    /// is not settable.  Use when you need very fast, but not fine-
    /// grained timestamps.  Requires per-architecture support, and
    /// probably also architecture support for this flag in the vdso(7).
    RealtimeCoarse = 5,

    /// (since Linux 2.6.32; Linux-specific)
    /// A faster but less precise version of CLOCK_MONOTONIC.  Use when
    /// you need very fast, but not fine-grained timestamps.  Requires
    /// per-architecture support, and probably also architecture support
    /// for this flag in the vdso(7).
    MonotonicCoarse = 6,

    /// (since Linux 2.6.39; Linux-specific)
    /// A nonsettable system-wide clock that is identical to
    /// CLOCK_MONOTONIC, except that it also includes any time that the
    /// system is suspended.  This allows applications to get a suspend-
    /// aware monotonic clock without having to deal with the
    /// complications of CLOCK_REALTIME, which may have discontinuities
    /// if the time is changed using settimeofday(2) or similar.
    Boottime = 7,

    /// (since Linux 3.0; Linux-specific)
    /// Like CLOCK_REALTIME, but not settable.  See timer_create(2) for
    /// further details.
    RealtimeAlarm = 8,

    /// (since Linux 3.0; Linux-specific)
    /// Like CLOCK_BOOTTIME.  See timer_create(2) for further details.
    BoottimeAlarm = 9,

    /// (since Linux 3.10; Linux-specific)
    /// A nonsettable system-wide clock derived from wall-clock time but
    /// ignoring leap seconds.  This clock does not experience
    /// discontinuities and backwards jumps caused by NTP inserting leap
    /// seconds as CLOCK_REALTIME does.
    ///
    /// The acronym TAI refers to International Atomic Time.
    InternationalAtomicTime = 11,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Timespec {
    tv_sec: i64,
    #[cfg(target_endian = "big")]
    __padding: i32,
    tv_nsec: u32,
    #[cfg(target_endian = "little")]
    __padding: i32,
}

cfg_if! {
    if #[cfg(any(
        target_arch = "arm",
        target_arch = "aarch64",
        target_arch = "mips",
        target_arch = "mips64",
        target_arch = "powerpc",
        target_arch = "powerpc64",
        target_arch = "riscv32",
        target_arch = "riscv64",
        target_arch = "s390x",
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "loongarch64"
    ))] {
        mod get_impl {
            use core::{
                mem::MaybeUninit,
                sync::atomic::{AtomicPtr, Ordering},
            };

            use linux_syscalls::{syscall, Errno};

            const UNINIT: *mut core::ffi::c_void = core::ptr::null_mut();
            const INIT_NULL: *mut core::ffi::c_void = 1 as _;
            static mut CLOCK_GETTIME_VSYSCALL: AtomicPtr<core::ffi::c_void> =
                AtomicPtr::new(core::ptr::null_mut());

            cfg_if::cfg_if! {
                if #[cfg(target_arch = "powerpc")] {
                    #[inline(always)]
                    fn vdso_clock_gettime(vdso: &linux_syscalls::env::Vdso) -> *const core::ffi::c_void {
                        vdso.clock_gettime64()
                    }
                } else {
                    #[inline(always)]
                    fn vdso_clock_gettime(vdso: &linux_syscalls::env::Vdso) -> *const core::ffi::c_void {
                        vdso.clock_gettime()
                    }
                }
            }

            #[inline(always)]
            fn clock_gettime_vsyscall(
            ) -> Option<extern "C" fn(super::ClockId, *mut super::Timespec) -> usize> {
                unsafe {
                    match CLOCK_GETTIME_VSYSCALL.load(Ordering::Relaxed) {
                        UNINIT => {
                            let ptr =
                                vdso_clock_gettime(linux_syscalls::env::vdso()) as *mut core::ffi::c_void;
                            if ptr.is_null() {
                                CLOCK_GETTIME_VSYSCALL.store(INIT_NULL, Ordering::Relaxed);
                                None
                            } else {
                                CLOCK_GETTIME_VSYSCALL.store(ptr, Ordering::Relaxed);
                                Some(core::mem::transmute(ptr))
                            }
                        }
                        INIT_NULL => None,
                        ptr => Some(core::mem::transmute(ptr)),
                    }
                }
            }

            pub fn clock_gettime(clockid: super::ClockId) -> Result<super::Timespec, Errno> {
                unsafe {
                    let mut buf = MaybeUninit::<super::Timespec>::uninit();
                    (*buf.as_mut_ptr()).__padding = 0;
                    if let Some(inner) = clock_gettime_vsyscall() {
                        Errno::from_ret(inner(clockid, buf.as_mut_ptr()))
                    } else {
                        syscall!(super::SYS_clock_gettime, clockid, buf.as_mut_ptr())
                    }
                    .map(|_| buf.assume_init())
                }
            }
        }
    } else {
        mod get_impl {
            #[inline(always)]
            pub fn clock_gettime(clockid: super::ClockId) -> Result<super::Timespec, Errno> {
                unsafe {
                    let mut buf = MaybeUninit::<super::Timespec>::uninit();
                    (*buf.as_mut_ptr()).__padding = 0;
                    syscall!(super::SYS_clock_gettime, clockid, buf.as_mut_ptr()).map(|_| buf.assume_init())
                }
            }
        }
    }
}

impl Timespec {
    #[inline(always)]
    pub const fn new(secs: i64, nsecs: u32) -> Self {
        Self {
            tv_sec: secs,
            tv_nsec: nsecs,
            __padding: 0,
        }
    }

    #[inline(always)]
    pub fn now(clockid: ClockId) -> Result<Self, Errno> {
        get_impl::clock_gettime(clockid)
    }

    #[inline(always)]
    pub const fn secs(&self) -> i64 {
        self.tv_sec
    }

    #[inline(always)]
    pub fn set_secs(&mut self, secs: i64) {
        self.tv_sec = secs;
    }

    #[inline(always)]
    pub const fn nsecs(&self) -> u32 {
        self.tv_nsec
    }

    #[inline(always)]
    pub fn set_nsecs(&mut self, nsecs: u32) {
        self.tv_nsec = nsecs;
    }

    #[inline]
    pub fn set_clock(&self) -> Result<(), Errno> {
        unsafe { syscall!([ro] SYS_clock_settime, ClockId::Realtime, self as *const Self) }
            .map(|_| ())
    }
}

impl Timespec {
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(0, 0)
    }

    #[inline(always)]
    pub const fn seconds(&self) -> i64 {
        self.secs()
    }

    #[inline(always)]
    pub fn set_seconds(&mut self, secs: i64) {
        self.set_secs(secs)
    }

    #[inline(always)]
    pub const fn nanosecs(&self) -> u32 {
        self.nsecs()
    }

    #[inline(always)]
    pub const fn nanoseconds(&self) -> u32 {
        self.nsecs()
    }

    #[inline(always)]
    pub fn set_nanosecs(&mut self, nsecs: u32) {
        self.set_nsecs(nsecs)
    }

    #[inline(always)]
    pub fn set_nanoseconds(&mut self, nsecs: u32) {
        self.set_nsecs(nsecs)
    }

    pub fn sub_timespec(&self, other: &Timespec) -> Result<Duration, Duration> {
        if self >= other {
            // NOTE(eddyb) two aspects of this `if`-`else` are required for LLVM
            // to optimize it into a branchless form (see also #75545):
            //
            // 1. `self.secs() - other.secs()` shows up as a common expression
            //    in both branches, i.e. the `else` must have its `- 1`
            //    subtraction after the common one, not interleaved with it
            //    (it used to be `self.secs() - 1 - other.secs()`)
            //
            // 2. the `Duration::new` call (or any other additional complexity)
            //    is outside of the `if`-`else`, not duplicated in both branches
            //
            // Ideally this code could be rearranged such that it more
            // directly expresses the lower-cost behavior we want from it.
            let (secs, nsecs) = if self.nsecs() >= other.nsecs() {
                (
                    (self.secs() - other.secs()) as u64,
                    self.nsecs() - other.nsecs(),
                )
            } else {
                (
                    (self.secs() - other.secs() - 1) as u64,
                    self.nsecs() + (NSEC_PER_SEC as u32) - other.nsecs(),
                )
            };

            Ok(Duration::new(secs, nsecs))
        } else {
            match other.sub_timespec(self) {
                Ok(d) => Err(d),
                Err(d) => Ok(d),
            }
        }
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<Timespec> {
        #[inline(always)]
        // fn checked_add_unsigned(a: i64, b: u64) -> Option<i64> {
        //     a.checked_add_unsigned(b)
        // }
        fn checked_add_unsigned(a: i64, b: u64) -> Option<i64> {
            let b = if b > I64_MAX {
                return None;
            } else {
                b as i64
            };
            a.checked_add(b)
        }

        let mut secs = checked_add_unsigned(self.secs(), other.as_secs())?;

        // Nano calculations can't overflow because nanos are <1B which fit
        // in a u32.
        let mut nsecs = other.subsec_nanos() + self.nsecs();
        if nsecs >= NSEC_PER_SEC as u32 {
            nsecs -= NSEC_PER_SEC as u32;
            secs = secs.checked_add(1)?;
        }
        Some(Timespec::new(secs, nsecs))
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<Timespec> {
        #[inline(always)]
        // fn checked_sub_unsigned(a: i64, b: u64) -> Option<i64> {
        //     a.checked_sub_unsigned(b)
        // }
        fn checked_sub_unsigned(a: i64, b: u64) -> Option<i64> {
            let b = if b > I64_MAX {
                return None;
            } else {
                b as i64
            };
            a.checked_sub(b)
        }

        let mut secs = checked_sub_unsigned(self.secs(), other.as_secs())?;

        // Similar to above, nanos can't overflow.
        let mut nsecs = self.nsecs() as i32 - other.subsec_nanos() as i32;
        if nsecs < 0 {
            nsecs += NSEC_PER_SEC as i32;
            secs = secs.checked_sub(1)?;
        }
        Some(Timespec::new(secs, nsecs as u32))
    }
}

impl Default for Timespec {
    #[inline]
    fn default() -> Self {
        Self::zero()
    }
}

impl fmt::Debug for Timespec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Timespec")
            .field("secs", &self.secs())
            .field("nsecs", &self.nsecs())
            .finish()
    }
}

impl PartialEq for Timespec {
    fn eq(&self, other: &Self) -> bool {
        self.secs() == other.secs() && self.nsecs() == other.nsecs()
    }
}

impl Eq for Timespec {}

impl PartialOrd for Timespec {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.secs().partial_cmp(&other.secs()) {
            Some(core::cmp::Ordering::Equal) => (),
            ord => return ord,
        }
        self.nsecs().partial_cmp(&other.nsecs())
    }
}

impl Ord for Timespec {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.secs().cmp(&other.secs()) {
            core::cmp::Ordering::Equal => (),
            ord => return ord,
        }
        self.nsecs().cmp(&other.nsecs())
    }
}

impl core::hash::Hash for Timespec {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.secs().hash(state);
        self.nsecs().hash(state);
    }
}
