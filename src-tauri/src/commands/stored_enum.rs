/// Declares a string enum that is stored (`body_json`, `auth_json`) or
/// imported (Postman) and must still load when the string is one we do not
/// know: any other string reads as `$fallback`.
///
/// `#[serde(other)]` would do that, but specta cannot export an enum that uses
/// it, and `deserialize_with` on the fields splits every containing type into
/// separate send and receive types in the bindings. So the exported enum stays
/// plain and its `Deserialize` goes through a private copy that has the
/// `other` variant.
macro_rules! stored_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident, fallback $fallback:ident {
            $($(#[$variant_meta:meta])* $variant:ident),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Eq, PartialEq, serde::Serialize, specta::Type)]
        #[serde(rename_all = "lowercase")]
        $vis enum $name {
            $($(#[$variant_meta])* $variant),+
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "lowercase")]
                enum Known {
                    $($variant),+,
                    #[serde(other)]
                    Other,
                }

                Ok(match Known::deserialize(deserializer)? {
                    $(Known::$variant => Self::$variant,)+
                    Known::Other => Self::$fallback,
                })
            }
        }
    };
}

pub(crate) use stored_enum;
