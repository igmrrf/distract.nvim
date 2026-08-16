use distract_engine::sprites::SpriteSet;

pub fn asset_entry(set: &SpriteSet, sorted_states: &[(String, Vec<usize>)]) -> serde_json::Value {
    let states: serde_json::Map<String, serde_json::Value> = sorted_states
        .iter()
        .map(|(state, indices)| {
            let filenames: Vec<serde_json::Value> = indices
                .iter()
                .enumerate()
                .map(|(offset, _)| serde_json::Value::String(format!("{}_{}", state, offset)))
                .collect();
            (state.clone(), serde_json::Value::Array(filenames))
        })
        .collect();

    serde_json::json!({
        "width": set.width,
        "height": set.height,
        "total_frames": set.frames.len(),
        "states": states,
    })
}
