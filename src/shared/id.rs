use crate::shared::errors::AppError;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

macro_rules! define_id {
    ($id_type: ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
        #[serde(into = "String")]
        pub struct $id_type(uuid::Uuid);

        impl $id_type {
            pub fn new() -> Self {
                Self(uuid::Uuid::new_v4())
            }

            pub fn raw(self) -> uuid::Uuid {
                self.0
            }

            pub fn to_string(&self) -> String {
                self.0.to_string()
            }
        }

        impl Default for $id_type {
            fn default() -> Self {
                Self::new()
            }
        }

        impl FromStr for $id_type {
            type Err = AppError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(uuid::Uuid::parse_str(s)?))
            }
        }

        impl From<String> for $id_type {
            fn from(s: String) -> Self {
                Self::from_str(&s).unwrap()
            }
        }

        impl From<uuid::Uuid> for $id_type {
            fn from(u: uuid::Uuid) -> Self {
                Self(u)
            }
        }

        impl From<Option<uuid::Uuid>> for $id_type {
            fn from(u: Option<uuid::Uuid>) -> Self {
                match u {
                    Some(uuid) => Self(uuid),
                    None => Self(uuid::Uuid::nil()),
                }
            }
        }

        impl From<$id_type> for uuid::Uuid {
            fn from(id: $id_type) -> Self {
                id.0
            }
        }

        impl std::fmt::Display for $id_type {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(
                    f,
                    "{}",
                    self.0
                        .as_simple()
                        .encode_lower(&mut uuid::Uuid::encode_buffer())
                )
            }
        }

        impl From<$id_type> for String {
            fn from(id: $id_type) -> Self {
                id.to_string()
            }
        }
    };
}

define_id!(WebSiteId);
define_id!(WebArticleId);
