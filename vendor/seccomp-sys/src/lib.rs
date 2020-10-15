extern crate libc;
/*
 * seccomp actions
 */
#[allow(non_camel_case_types)]
pub type scmp_filter_ctx = libc::c_void;

/**
 * Error retern value
 */
pub const __NR_SCMP_ERROR: libc::c_int = -1;

/**
 * Kill the calling thread
 */
pub const SCMP_ACT_KILL: u32  = 0x00000000;
/**
 * Kill the calling process
 */
pub const SCMP_ACT_KILL_PROCESS: u32 = 0x80000000;
/**
 * Throw a SIGSYS signal
 */
pub const SCMP_ACT_TRAP: u32 = 0x00030000;
/**
 * Return the specified error code
 */
#[allow(non_snake_case)]
pub fn SCMP_ACT_ERRNO(x: u32) -> u32 { 0x00050000 | ((x) & 0x0000ffff) }
/**
 * Notify a tracing process with the specified value
 */
#[allow(non_snake_case)]
pub fn SCMP_ACT_TRACE(x: u32) -> u32 { 0x7ff00000 | ((x) & 0x0000ffff) }
/**
 * Allow the syscall to be executed
 */
pub const SCMP_ACT_ALLOW: u32 = 0x7fff0000;

/**
 * Filter attributes
 */
#[allow(non_camel_case_types)]
#[derive(Debug,Clone,Copy)]
#[repr(C)]
pub enum scmp_filter_attr {
    _SCMP_FLTATR_MIN,
    SCMP_FLTATR_ACT_DEFAULT, /** default filter action */
    SCMP_FLTATR_ACT_BADARCH, /** bad architecture action */
    SCMP_FLTATR_CTL_NNP, /** set NO_NEW_PRIVS on filter load */
    _SCMP_FLTATR_MAX,
}

/**
 * Comparison operators
 */
#[allow(non_camel_case_types)]
#[derive(Debug,Clone,Copy)]
#[repr(C)]
pub enum scmp_compare {
        _SCMP_CMP_MIN = 0,
        SCMP_CMP_NE = 1,                /** not equal */
        SCMP_CMP_LT = 2,                /** less than */
        SCMP_CMP_LE = 3,                /** less than or equal */
        SCMP_CMP_EQ = 4,                /** equal */
        SCMP_CMP_GE = 5,                /** greater than or equal */
        SCMP_CMP_GT = 6,                /** greater than */
        SCMP_CMP_MASKED_EQ = 7,         /** masked equality */
        _SCMP_CMP_MAX,
}

/**
 * Architecutres
 */
#[allow(non_camel_case_types)]
#[derive(Debug,Clone,Copy)]
#[repr(C)]
pub enum scmp_arch {
    SCMP_ARCH_NATIVE = 0x0,
    SCMP_ARCH_X86 = 0x40000003,
    SCMP_ARCH_X86_64 = 0xc000003e,
    SCMP_ARCH_X32 = 0x4000003e,
    SCMP_ARCH_ARM = 0x40000028,
    SCMP_ARCH_AARCH64 = 0xc00000b7,
    SCMP_ARCH_MIPS = 0x8,
    SCMP_ARCH_MIPS64 = 0x80000008,
    SCMP_ARCH_MIPS64N32 = 0xa0000008,
    SCMP_ARCH_MIPSEL = 0x40000008,
    SCMP_ARCH_MIPSEL64 = 0xc0000008,
    SCMP_ARCH_MIPSEL64N32 = 0xe0000008,
    SCMP_ARCH_PPC = 0x14,
    SCMP_ARCH_PPC64 = 0x80000015,
    SCMP_ARCH_PPC64LE = 0xc0000015,
    SCMP_ARCH_S390 = 0x16,
    SCMP_ARCH_S390X = 0x80000016,
}

/**
 * Argument datum
 */
#[allow(non_camel_case_types)]
pub type scmp_datum_t = u64;

/**
 * Argument / Value comparison definition
 */
#[derive(Debug)]
#[repr(C)]
pub struct scmp_arg_cmp {
        pub arg: libc::c_uint,        /** argument number, starting at 0 */
        pub op: scmp_compare,       /** the comparison op, e.g. SCMP_CMP_* */
        pub datum_a: scmp_datum_t,
        pub datum_b: scmp_datum_t,
}

#[link(name = "seccomp")]
extern {
    /**
     * Initialize the filter state
     *
     * @param def_action the default filter action
     *
     * This function initializes the internal seccomp filter state and should
     * be called before any other functions in this library to ensure the filter
     * state is initialized.  Returns a filter context on success, NULL on failure.
     *
     */
    pub fn seccomp_init(def_action: u32) -> *mut scmp_filter_ctx;
    /**
     * Reset the filter state
     *
     * @param ctx the filter context
     * @param def_action the default filter action
     *
     * This function resets the given seccomp filter state and ensures the
     * filter state is reinitialized.  This function does not reset any seccomp
     * filters already loaded into the kernel.  Returns zero on success, negative
     * values on failure.
     *
     */
    pub fn seccomp_reset(ctx: *mut scmp_filter_ctx, def_action: u32) -> libc::c_int;
    /**
     * Destroys the filter state and releases any resources
     *
     * @param ctx the filter context
     *
     * This functions destroys the given seccomp filter state and releases any
     * resources, including memory, associated with the filter state.  This
     * function does not reset any seccomp filters already loaded into the kernel.
     * The filter context can no longer be used after calling this function.
     *
     */
    pub fn seccomp_release(ctx: *mut scmp_filter_ctx);

    /**
     * Adds an architecture to the filter
     * @param ctx the filter context
     * @param arch_token the architecture token, e.g. SCMP_ARCH_*
     *
     * This function adds a new architecture to the given seccomp filter context.
     * Any new rules added after this function successfully returns will be added
     * to this architecture but existing rules will not be added to this
     * architecture.  If the architecture token is SCMP_ARCH_NATIVE then the native
     * architecture will be assumed.  Returns zero on success, negative values on
     * failure.
     *
     */
    pub fn seccomp_arch_add(ctx: *mut scmp_filter_ctx, arch_token: u32) -> libc::c_int;

    /**
     * Removes an architecture from the filter
     * @param ctx the filter context
     * @param arch_token the architecture token, e.g. SCMP_ARCH_*
     *
     * This function removes an architecture from the given seccomp filter context.
     * If the architecture token is SCMP_ARCH_NATIVE then the native architecture
     * will be assumed.  Returns zero on success, negative values on failure.
     *
     */
    pub fn seccomp_arch_remove(ctx: *mut scmp_filter_ctx, arch_token: u32)-> libc::c_int;

    /**
     * Loads the filter into the kernel
     *
     * @param ctx the filter context
     *
     * This function loads the given seccomp filter context into the kernel.  If
     * the filter was loaded correctly, the kernel will be enforcing the filter
     * when this function returns.  Returns zero on success, negative values on
     * error.
     *
     */
    pub fn seccomp_load(ctx: *const scmp_filter_ctx) -> libc::c_int;

    /**
     * Get the value of a filter attribute
     *
     * @param ctx the filter context
     * @param attr the filter attribute name
     * @param value the filter attribute value
     *
     * This function fetches the value of the given attribute name and returns it
     * via @value.  Returns zero on success, negative values on failure.
     *
     */
    pub fn seccomp_attr_get(ctx: *const scmp_filter_ctx,
                         attr: scmp_filter_attr, value: *mut u32) -> libc::c_int;

    /**
     * Set the value of a filter attribute
     *
     * @param ctx the filter context
     * @param attr the filter attribute name
     * @param value the filter attribute value
     *
     * This function sets the value of the given attribute.  Returns zero on
     * success, negative values on failure.
     *
     */
    pub fn seccomp_attr_set(ctx: *mut scmp_filter_ctx,
                         attr: scmp_filter_attr, value: u32) -> libc::c_int;

    /**
     * Resolve a syscall name to a number
     * @param name the syscall name
     *
     * Resolve the given syscall name to the syscall number.  Returns the syscall
     * number on success, including negative pseudo syscall numbers (e.g. __PNR_*);
     * returns __NR_SCMP_ERROR on failure.
     *
     */
    pub fn seccomp_syscall_resolve_name(name: *const libc::c_char) -> libc::c_int;

    /**
     * Set the priority of a given syscall
     *
     * @param ctx the filter context
     * @param syscall the syscall number
     * @param priority priority value, higher value == higher priority
     *
     * This function sets the priority of the given syscall; this value is used
     * when generating the seccomp filter code such that higher priority syscalls
     * will incur less filter code overhead than the lower priority syscalls in the
     * filter.  Returns zero on success, negative values on failure.
     *
     */
    pub fn seccomp_syscall_priority(ctx: *mut scmp_filter_ctx,
                                 syscall: libc::c_int, priority: u8) -> libc::c_int;

    /**
     * Add a new rule to the filter
     *
     * @param ctx the filter context
     * @param action the filter action
     * @param syscall the syscall number
     * @param arg_cnt the number of argument filters in the argument filter chain
     * @param ... scmp_arg_cmp structs (use of SCMP_ARG_CMP() recommended)
     *
     * This function adds a series of new argument/value checks to the seccomp
     * filter for the given syscall; multiple argument/value checks can be
     * specified and they will be chained together (AND'd together) in the filter.
     * If the specified rule needs to be adjusted due to architecture specifics it
     * will be adjusted without notification.  Returns zero on success, negative
     * values on failure.
     *
     */
    pub fn seccomp_rule_add(ctx: *mut scmp_filter_ctx,
                         action: u32, syscall: libc::c_int, arg_cnt: libc::c_uint, ...) -> libc::c_int;



    /**
     * Add a new rule to the filter
     *
     * @param ctx the filter context
     * @param action the filter action
     * @param syscall the syscall number
     * @param arg_cnt the number of elements in the arg_array parameter
     * @param arg_array array of scmp_arg_cmp structs
     *
     * This function adds a series of new argument/value checks to the seccomp
     * filter for the given syscall; multiple argument/value checks can be
     * specified and they will be chained together (AND'd together) in the filter.
     * If the specified rule needs to be adjusted due to architecture specifics it
     * will be adjusted without notification.  Returns zero on success, negative
     * values on failure.
     *
     */
    pub fn seccomp_rule_add_array(ctx: *mut scmp_filter_ctx,
        action: u32, syscall: libc::c_int, arg_cnt: libc::c_uint,
        arg_array: *const scmp_arg_cmp) -> libc::c_int;

    /**
     * Add a new rule to the filter
     *
     * @param ctx the filter context
     * @param action the filter action
     * @param syscall the syscall number
     * @param arg_cnt the number of argument filters in the argument filter chain
     * @param ... scmp_arg_cmp structs (use of SCMP_ARG_CMP() recommended)
     *
     * This function adds a series of new argument/value checks to the seccomp
     * filter for the given syscall; multiple argument/value checks can be
     * specified and they will be chained together (AND'd together) in the filter.
     * If the specified rule can not be represented on the architecture the
     * function will fail.  Returns zero on success, negative values on failure.
     *
     */
    pub fn seccomp_rule_add_exact(ctx: *mut scmp_filter_ctx, action: u32,
                               syscall: libc::c_int, arg_cnt: libc::c_uint, ...) -> libc::c_int;

    /**
     * Add a new rule to the filter
     *
     * @param ctx the filter context
     * @param action the filter action
     * @param syscall the syscall number
     * @param arg_cnt  the number of elements in the arg_array parameter
     * @param arg_array array of scmp_arg_cmp structs
     *
     * This function adds a series of new argument/value checks to the seccomp
     * filter for the given syscall; multiple argument/value checks can be
     * specified and they will be chained together (AND'd together) in the filter.
     * If the specified rule can not be represented on the architecture the
     * function will fail.  Returns zero on success, negative values on failure.
     *
     */
    pub fn seccomp_rule_add_exact_array(ctx: *mut scmp_filter_ctx,
                                        action: u32, syscall: libc::c_int, arg_cnt: libc::c_uint,
                                        arg_array: *const scmp_arg_cmp) -> libc::c_int;

    /**
     * Generate seccomp Pseudo Filter Code (PFC) and export it to a file
     *
     * @param ctx the filter context
     * @param fd the destination fd
     *
     * This function generates seccomp Pseudo Filter Code (PFC) and writes it to
     * the given fd.  Returns zero on success, negative values on failure.
     *
     */
    pub fn seccomp_export_pfc(ctx: *const scmp_filter_ctx, fd: libc::c_int) -> libc::c_int;

    /**
     * Generate seccomp Berkley Packet Filter (BPF) code and export it to a file
     *
     * @param ctx the filter context
     * @param fd the destination fd
     *
     * This function generates seccomp Berkley Packer Filter (BPF) code and writes
     * it to the given fd.  Returns zero on success, negative values on failure.
     *
     */
    pub fn seccomp_export_bpf(ctx: *const scmp_filter_ctx, fd: libc::c_int) -> libc::c_int;
}

#[test]
fn does_it_even_work() {
    unsafe {
        let context = seccomp_init(SCMP_ACT_ALLOW);
        let comparator = scmp_arg_cmp {
            arg: 0,
            op: scmp_compare::SCMP_CMP_EQ,
            datum_a: 1000,
            datum_b: 0,
        };
        assert!(seccomp_rule_add(context, SCMP_ACT_KILL, 105, 1, comparator) == 0);
        assert!(seccomp_load(context) == 0);
        //assert!(libc::setuid(1000) == 0);
    }
}
