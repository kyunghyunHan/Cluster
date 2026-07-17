use serde::{Deserialize, Serialize};

macro_rules! numeric_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub(crate) struct $name(pub(crate) u64);

        impl From<u64> for $name {
            fn from(value: u64) -> Self {
                Self(value)
            }
        }

        impl From<$name> for u64 {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

numeric_id!(JunctionId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_junction_id_keeps_legacy_numeric_json_shape() {
        let json = serde_json::to_string(&JunctionId(42)).expect("serialize typed id");
        assert_eq!(json, "42");
        assert_eq!(
            serde_json::from_str::<JunctionId>(&json).expect("deserialize typed id"),
            JunctionId(42)
        );
    }
}
