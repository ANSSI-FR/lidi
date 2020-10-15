use seccomp_sys::*;
use strum_macros::EnumString;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString)]
pub enum Cmp {
    Ne,
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
    MaskedEq,
}

impl Into<scmp_compare> for Cmp {
    fn into(self) -> scmp_compare {
        use self::Cmp::*;
        use scmp_compare::*;
        match self {
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

#[derive(Debug, Clone)]
pub struct Comparator {
    arg: u32,
    op: Cmp,
    datum_a: u64,
    datum_b: u64,
}

impl Comparator {
    pub fn new(arg: u32, op: Cmp, datum_a: u64, datum_b: Option<u64>) -> Self {
        Self {
            arg,
            op,
            datum_a,
            datum_b: datum_b.unwrap_or(0_u64),
        }
    }
}

impl Into<scmp_arg_cmp> for Comparator {
    fn into(self) -> scmp_arg_cmp {
        scmp_arg_cmp {
            arg: self.arg,
            op: self.op.into(),
            datum_a: self.datum_a,
            datum_b: self.datum_b,
        }
    }
}
