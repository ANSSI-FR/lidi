extern crate libc;
extern crate seccomp_sys;

fn main() {
    unsafe {
        let context = seccomp_sys::seccomp_init(seccomp_sys::SCMP_ACT_ALLOW);
        let comparator = seccomp_sys::scmp_arg_cmp {
            arg: 0,
            op: seccomp_sys::scmp_compare::SCMP_CMP_EQ,
            datum_a: 1000,
            datum_b: 0,
        }; /* arg[0] equals 1000 */

        let syscall_number = 105; /* setuid on x86_64 */
        assert!(seccomp_sys::seccomp_rule_add(context, seccomp_sys::SCMP_ACT_KILL, syscall_number, 1, comparator) == 0);
        assert!(seccomp_sys::seccomp_load(context) == 0);
        assert!(libc::setuid(1000) == 0); /* process would be killed here */
    }
}
