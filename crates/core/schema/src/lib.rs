pub mod neoutl {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/neoutl.v1.rs"));
    }
}

pub use neoutl::v1::*;
