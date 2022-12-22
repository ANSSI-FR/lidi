use seccomp_sys::*;
use strum_macros::EnumString;

/// An enum for `!=`, `<`, `<=`, `==`, `>=`, `>`
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString)]
pub enum Cmp {
    /// Not equal, `!=`
    Ne,
    /// Lower-than, `<`
    Lt,
    /// Lower-or-equal, `<=`
    Le,
    /// Equal, `==`
    Eq,
    /// Greater-or-equal, `>=`
    Ge,
    /// Greater-than, `>`
    Gt,
    /// Equal for the masked argument value, the mask is provided in `datum_a` of the [`Comparator`](struct.Comparator.html).
    MaskedEq,
}

impl From<Cmp> for scmp_compare {
    fn from(cmp: Cmp) -> scmp_compare {
        use self::Cmp::*;
        use scmp_compare::*;
        match cmp {
            Ne => SCMP_CMP_NE,
            Lt => SCMP_CMP_LT,
            Le => SCMP_CMP_LE,
            Eq => SCMP_CMP_EQ,
            Ge => SCMP_CMP_GE,
            Gt => SCMP_CMP_GT,
            MaskedEq => SCMP_CMP_MASKED_EQ,
        }
    }
}

/// A compare rule to restrict an argument syscall
#[derive(Debug, Clone)]
pub struct Comparator {
    arg: u32,
    op: Cmp,
    datum_a: u64,
    datum_b: u64,
}

impl Comparator {
    /// Set a constraint for a syscall argument.
    /// - The first argument is the syscall argument index, `0` would be the first argument.
    /// - The second argument selects a compare operation like equals-to, greather-than, etc.
    /// - The third argument is the value it's going to be compared to.
    /// - The forth argument is only used when using Cmp::MaskedEq, where `datum_a` is used as a mask and `datum_b` is the value the result is compared to.
    pub fn new(arg: u32, op: Cmp, datum_a: u64, datum_b: Option<u64>) -> Self {
        Self {
            arg,
            op,
            datum_a,
            datum_b: datum_b.unwrap_or(0_u64),
        }
    }
}

impl From<Comparator> for scmp_arg_cmp {
    fn from(cmp: Comparator) -> scmp_arg_cmp {
        scmp_arg_cmp {
            arg: cmp.arg,
            op: cmp.op.into(),
            datum_a: cmp.datum_a,
            datum_b: cmp.datum_b,
        }
    }
}
