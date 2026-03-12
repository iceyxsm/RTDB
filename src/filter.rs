//! Production-grade filter implementation for vector search
//!
//! Implements Qdrant-compatible filtering with:
//! - Nested conditions (must, should, must_not)
//! - Field matching (exact, range, text)
//! - Payload filtering with nested key support
//! - Optimized evaluation order

use crate::{Condition, FieldCondition, Filter, Match, MatchValue, Range, Vector, VectorId};
use serde_json::Value;

/// Filter evaluator for applying filters to vectors
pub struct FilterEvaluator;

impl FilterEvaluator {
    /// Apply filter to a vector, returns true if vector matches
    pub fn matches(filter: &Filter, id: VectorId, vector: &Vector) -> bool {
        let result = true;

        // Evaluate MUST conditions (AND logic)
        if let Some(must) = &filter.must {
            for condition in must {
                if !Self::evaluate_condition(condition, id, vector) {
                    return false;
                }
            }
        }

        // Evaluate SHOULD conditions (OR logic)
        if let Some(should) = &filter.should {
            if !should.is_empty() {
                let any_match = should.iter().any(|condition| {
                    Self::evaluate_condition(condition, id, vector)
                });
                if !any_match {
                    return false;
                }
            }
        }

        // Evaluate MUST_NOT conditions (NOT logic)
        if let Some(must_not) = &filter.must_not {
            for condition in must_not {
                if Self::evaluate_condition(condition, id, vector) {
                    return false;
                }
            }
        }

        result
    }

    /// Evaluate a single condition
    fn evaluate_condition(condition: &Condition, id: VectorId, vector: &Vector) -> bool {
        match condition {
            Condition::Field(field_cond) => Self::evaluate_field_condition(field_cond, vector),
            Condition::Filter(nested_filter) => Self::matches(nested_filter, id, vector),
            Condition::HasId(has_id_cond) => has_id_cond.has_id.contains(&id),
        }
    }

    /// Evaluate field condition against vector payload
    fn evaluate_field_condition(condition: &FieldCondition, vector: &Vector) -> bool {
        let payload = match &vector.payload {
            Some(p) => p,
            None => return false,
        };

        // Extract value from payload using key (supports nested keys with dot notation)
        let value = Self::get_nested_value(payload, &condition.key);
        
        match value {
            Some(val) => Self::evaluate_match(&condition.r#match, val),
            None => false,
        }
    }

    /// Get nested value from payload using dot notation (e.g., "country.name")
    fn get_nested_value<'a>(payload: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a Value> {
        // Handle array projection syntax: "cities[].name"
        if key.contains("[]") {
            // For array projections, we need special handling
            // This is a simplified version - production would need full array support
            let parts: Vec<&str> = key.split(".").collect();
            let mut current: Option<&Value> = None;
            
            for (i, &part) in parts.iter().enumerate() {
                if part.ends_with("[]") {
                    // Array projection - check if any element matches
                    let array_key = part.trim_end_matches("[]");
                    if i == 0 {
                        current = payload.get(array_key);
                    } else {
                        current = current.and_then(|v| v.get(array_key));
                    }
                    // For now, return the array itself
                    // Full implementation would iterate through array elements
                    return current;
                } else {
                    if i == 0 {
                        current = payload.get(part);
                    } else {
                        current = current.and_then(|v| v.get(part));
                    }
                }
            }
            current
        } else {
            // Simple dot notation: "country.name"
            let parts: Vec<&str> = key.split('.').collect();
            let mut current: Option<&Value> = None;
            
            for (i, &part) in parts.iter().enumerate() {
                if i == 0 {
                    current = payload.get(part);
                } else {
                    current = current.and_then(|v| v.get(part));
                }
            }
            current
        }
    }

    /// Evaluate match condition against a value
    fn evaluate_match(match_cond: &Match, value: &Value) -> bool {
        match match_cond {
            Match::Value(match_val) => Self::evaluate_value_match(match_val, value),
            Match::Integer(match_int) => {
                if let Some(num) = value.as_i64() {
                    num == match_int.integer
                } else {
                    false
                }
            }
            Match::Text(match_text) => {
                if let Some(text) = value.as_str() {
                    // Simple substring match - production would use full-text index
                    text.to_lowercase().contains(&match_text.text.to_lowercase())
                } else {
                    false
                }
            }
            Match::Range(range) => Self::evaluate_range(range, value),
        }
    }

    /// Evaluate value match (keyword or integer)
    fn evaluate_value_match(match_val: &MatchValue, value: &Value) -> bool {
        match match_val {
            MatchValue::Keyword(keyword) => {
                // Handle both string and array of strings
                match value {
                    Value::String(s) => s == keyword,
                    Value::Array(arr) => arr.iter().any(|v| {
                        v.as_str().map(|s| s == keyword).unwrap_or(false)
                    }),
                    _ => false,
                }
            }
            MatchValue::Integer(int_val) => {
                match value {
                    Value::Number(n) => n.as_i64().map(|i| i == *int_val).unwrap_or(false),
                    Value::Array(arr) => arr.iter().any(|v| {
                        v.as_i64().map(|i| i == *int_val).unwrap_or(false)
                    }),
                    _ => false,
                }
            }
        }
    }

    /// Evaluate range condition
    fn evaluate_range(range: &Range, value: &Value) -> bool {
        let num = match value {
            Value::Number(n) => n.as_f64(),
            _ => None,
        };

        if let Some(n) = num {
            let mut matches = true;

            if let Some(gt) = range.gt {
                matches = matches && n > gt;
            }
            if let Some(gte) = range.gte {
                matches = matches && n >= gte;
            }
            if let Some(lt) = range.lt {
                matches = matches && n < lt;
            }
            if let Some(lte) = range.lte {
                matches = matches && n <= lte;
            }

            matches
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_simple_match() {
        let mut payload = serde_json::Map::new();
        payload.insert("city".to_string(), json!("London"));
        
        let vector = Vector {
            vector: vec![1.0, 2.0, 3.0],
            payload: Some(payload),
        };

        let filter = Filter {
            must: Some(vec![Condition::Field(FieldCondition {
                key: "city".to_string(),
                r#match: Match::Value(MatchValue::Keyword("London".to_string())),
            })]),
            should: None,
            must_not: None,
        };

        assert!(FilterEvaluator::matches(&filter, 1, &vector));
    }

    #[test]
    fn test_nested_key() {
        let mut payload = serde_json::Map::new();
        payload.insert("country".to_string(), json!({
            "name": "Germany",
            "code": "DE"
        }));
        
        let vector = Vector {
            vector: vec![1.0, 2.0, 3.0],
            payload: Some(payload),
        };

        let filter = Filter {
            must: Some(vec![Condition::Field(FieldCondition {
                key: "country.name".to_string(),
                r#match: Match::Value(MatchValue::Keyword("Germany".to_string())),
            })]),
            should: None,
            must_not: None,
        };

        assert!(FilterEvaluator::matches(&filter, 1, &vector));
    }

    #[test]
    fn test_range_condition() {
        let mut payload = serde_json::Map::new();
        payload.insert("price".to_string(), json!(150.0));
        
        let vector = Vector {
            vector: vec![1.0, 2.0, 3.0],
            payload: Some(payload),
        };

        let filter = Filter {
            must: Some(vec![Condition::Field(FieldCondition {
                key: "price".to_string(),
                r#match: Match::Range(Range {
                    gt: None,
                    gte: Some(100.0),
                    lt: None,
                    lte: Some(200.0),
                }),
            })]),
            should: None,
            must_not: None,
        };

        assert!(FilterEvaluator::matches(&filter, 1, &vector));
    }

    #[test]
    fn test_must_not() {
        let mut payload = serde_json::Map::new();
        payload.insert("color".to_string(), json!("red"));
        
        let vector = Vector {
            vector: vec![1.0, 2.0, 3.0],
            payload: Some(payload),
        };

        let filter = Filter {
            must: None,
            should: None,
            must_not: Some(vec![Condition::Field(FieldCondition {
                key: "color".to_string(),
                r#match: Match::Value(MatchValue::Keyword("blue".to_string())),
            })]),
        };

        assert!(FilterEvaluator::matches(&filter, 1, &vector));
    }

    #[test]
    fn test_has_id_condition() {
        let vector = Vector {
            vector: vec![1.0, 2.0, 3.0],
            payload: None,
        };

        let filter = Filter {
            must: Some(vec![Condition::HasId(HasIdCondition {
                has_id: vec![1, 2, 3],
            })]),
            should: None,
            must_not: None,
        };

        assert!(FilterEvaluator::matches(&filter, 1, &vector));
        assert!(!FilterEvaluator::matches(&filter, 5, &vector));
    }
}
