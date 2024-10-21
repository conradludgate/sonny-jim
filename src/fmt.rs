use core::fmt::{self, Debug};

use crate::{Arena, LeafValue, Value, ValueKind};

impl Arena<'_> {
    pub fn debug_fmt_value(&self, value: &Value, f: &mut fmt::Formatter) -> fmt::Result {
        FmtValue { arena: self, value }.fmt(f)
    }
}

struct FmtValue<'a, 's, 'v> {
    arena: &'a Arena<'s>,
    value: &'v Value,
}

impl fmt::Debug for FmtValue<'_, '_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.value.kind {
            ValueKind::Leaf(leaf_value) => match leaf_value {
                LeafValue::Bool(true) => f.write_str("true"),
                LeafValue::Bool(false) => f.write_str("false"),
                LeafValue::Null => f.write_str("null"),
                LeafValue::String | LeafValue::Number => f.write_str(
                    &self.arena.scratch.src
                        [self.value.span.start as usize..self.value.span.end as usize],
                ),
            },
            ValueKind::Object(object) => {
                let mut f = f.debug_map();

                let keys = &self.arena.keys[object.keys.start as usize..object.keys.end as usize];
                let values =
                    &self.arena.values[object.values.start as usize..object.values.end as usize];
                for (k, v) in core::iter::zip(keys, values) {
                    let k = &self.arena[k];
                    f.entry(
                        &k,
                        &FmtValue {
                            arena: self.arena,
                            value: v,
                        },
                    );
                }

                f.finish()
            }
            ValueKind::Array(array) => {
                let mut f = f.debug_list();

                let values =
                    &self.arena.values[array.values.start as usize..array.values.end as usize];
                for v in values {
                    f.entry(&FmtValue {
                        arena: self.arena,
                        value: v,
                    });
                }

                f.finish()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{fmt::FmtValue, Arena};

    #[test]
    fn snapshot() {
        let data = r#"{
            "definitions": {
                "io.k8s.api.admissionregistration.v1.AuditAnnotation": {
                    "description": "AuditAnnotation describes how to produce an audit annotation for an API request.",
                    "properties": {
                        "key": {
                            "description": "key specifies the audit annotation key. The audit annotation keys of a ValidatingAdmissionPolicy must be unique. The key must be a qualified name ([A-Za-z0-9][-A-Za-z0-9_.]*) no more than 63 bytes in length.\n\nThe key is combined with the resource name of the ValidatingAdmissionPolicy to construct an audit annotation key: \"{ValidatingAdmissionPolicy name}/{key}\".\n\nIf an admission webhook uses the same resource name as this ValidatingAdmissionPolicy and the same audit annotation key, the annotation key will be identical. In this case, the first annotation written with the key will be included in the audit event and all subsequent annotations with the same key will be discarded.\n\nRequired.",
                            "type": "string"
                        },
                        "valueExpression": {
                            "description": "valueExpression represents the expression which is evaluated by CEL to produce an audit annotation value. The expression must evaluate to either a string or null value. If the expression evaluates to a string, the audit annotation is included with the string value. If the expression evaluates to null or empty string the audit annotation will be omitted. The valueExpression may be no longer than 5kb in length. If the result of the valueExpression is more than 10kb in length, it will be truncated to 10kb.\n\nIf multiple ValidatingAdmissionPolicyBinding resources match an API request, then the valueExpression will be evaluated for each binding. All unique values produced by the valueExpressions will be joined together in a comma-separated list.\n\nRequired.",
                            "type": "string"
                        }
                    },
                    "required": [
                        "key",
                        "valueExpression"
                    ],
                    "type": "object"
                }
            }
        }"#;

        let mut arena = Arena::new(data);
        let value = crate::parse(&mut arena).unwrap();
        insta::assert_debug_snapshot!(FmtValue {
            arena: &arena,
            value: &value
        });
    }
}
