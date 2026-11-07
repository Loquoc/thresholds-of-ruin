//! Test-only access to payloads in canonical checkpoint directories.
use serde_json::Value;

pub fn actor<'a>(shared: &'a Value, table: usize, id: &str) -> &'a Value {
    let record = shared["actors"][table][id].as_u64().unwrap() as usize;
    &shared["actor_records"][record]
}

pub fn actor_mut<'a>(shared: &'a mut Value, table: usize, id: &str) -> &'a mut Value {
    let original = shared["actors"][table][id].as_u64().unwrap() as usize;
    let references = shared["actors"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|directory| directory.as_object().unwrap().values())
        .filter(|index| index.as_u64().unwrap() as usize == original)
        .count();
    // The old fixture edited one actor binding in one pooled directory. Keep
    // that isolation now that different directories can share its payload.
    let record = if references > 1 {
        let value = shared["actor_records"][original].clone();
        let records = shared["actor_records"].as_array_mut().unwrap();
        let record = records.len();
        records.push(value);
        shared["actors"][table][id] = serde_json::json!(record);
        record
    } else {
        original
    };
    &mut shared["actor_records"][record]
}

#[test]
fn actor_mut_isolates_a_directory_binding_without_orphaning_records() {
    let mut shared = serde_json::json!({
        "actors": [{"1": 0, "2": 1}, {"1": 0}],
        "actor_records": [{"ready_at": 10}, {"ready_at": 20}]
    });
    actor_mut(&mut shared, 1, "1")["ready_at"] = serde_json::json!(11);
    assert_eq!(actor(&shared, 0, "1")["ready_at"], 10);
    assert_eq!(actor(&shared, 0, "2")["ready_at"], 20);
    assert_eq!(actor(&shared, 1, "1")["ready_at"], 11);
    assert_eq!(shared["actor_records"].as_array().unwrap().len(), 3);
    actor_mut(&mut shared, 1, "1")["ready_at"] = serde_json::json!(12);
    assert_eq!(shared["actor_records"].as_array().unwrap().len(), 3);
    assert_eq!(actor(&shared, 1, "1")["ready_at"], 12);
}
