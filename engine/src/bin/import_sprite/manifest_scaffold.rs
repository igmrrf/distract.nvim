pub struct StateSpec {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

pub fn parse_states_arg(raw: &str, total_frames: usize) -> Result<Vec<StateSpec>, String> {
    let mut states = Vec::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        let (name, range) = entry
            .split_once(':')
            .ok_or_else(|| format!("'{entry}' is not name:start-end"))?;
        let (start_text, end_text) = range
            .split_once('-')
            .ok_or_else(|| format!("'{range}' is not start-end"))?;
        let start: usize = start_text
            .trim()
            .parse()
            .map_err(|_| format!("'{start_text}' is not a number"))?;
        let end: usize = end_text
            .trim()
            .parse()
            .map_err(|_| format!("'{end_text}' is not a number"))?;
        if end < start || end >= total_frames {
            return Err(format!(
                "state '{name}' range {start}-{end} is out of bounds for {total_frames} frames"
            ));
        }
        states.push(StateSpec {
            name: name.to_string(),
            start,
            end,
        });
    }
    if states.is_empty() {
        return Err("no states parsed".to_string());
    }
    Ok(states)
}

pub fn default_state(total_frames: usize) -> Vec<StateSpec> {
    vec![StateSpec {
        name: "default".to_string(),
        start: 0,
        end: total_frames.saturating_sub(1),
    }]
}

pub struct ManifestParams<'a> {
    pub name: &'a str,
    pub sheet_path: &'a str,
    pub native_path: &'a str,
    pub frame_width: u32,
    pub frame_height: u32,
    pub columns: u32,
    pub rows: u32,
    pub states: &'a [StateSpec],
    pub fps: f32,
}

fn render_state(state: &StateSpec, fps: f32) -> String {
    let frames: Vec<String> = (state.start..=state.end)
        .map(|frame| frame.to_string())
        .collect();
    format!(
        "    {name} = {{
      animation = {{ frames = {{ {frames} }}, fps = {fps:.1}, loop_anim = true, flip_x = false }},
      physics = {{ target_vx = 2.0, target_vy = 0.0, wrap_mode = \"wrap\" }}, -- placeholder: tune per asset
      transitions = {{ on_event = {{}} }},
    }},
",
        name = state.name,
        frames = frames.join(", "),
        fps = fps,
    )
}

pub fn render_manifest(params: &ManifestParams) -> String {
    let initial_state = params
        .states
        .first()
        .map(|state| state.name.as_str())
        .unwrap_or("default");
    let states_lua: String = params
        .states
        .iter()
        .map(|state| render_state(state, params.fps))
        .collect();

    format!(
        "local M = {{
  name = \"{name}\",
  asset_type = \"sprite\",
  spritesheet = {{
    path = \"{sheet_path}\",
    native_path = \"{native_path}\",
    frame_width = {frame_width},
    frame_height = {frame_height},
    columns = {columns},
    rows = {rows},
  }},
  anchor = \"bottom\",
  initial_state = \"{initial_state}\",
  locomotion = \"grounded\",
  capabilities = {{ locomotion = {{ \"grounded\" }} }},
  states = {{
{states_lua}  }},
}}

return M
",
        name = params.name,
        sheet_path = params.sheet_path,
        native_path = params.native_path,
        frame_width = params.frame_width,
        frame_height = params.frame_height,
        columns = params.columns,
        rows = params.rows,
        initial_state = initial_state,
        states_lua = states_lua,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_states_arg_reads_one_or_more_ranges() {
        let states = parse_states_arg("walk:0-31,idle:32-35", 36).expect("parse");
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].name, "walk");
        assert_eq!(states[0].start, 0);
        assert_eq!(states[0].end, 31);
        assert_eq!(states[1].name, "idle");
    }

    #[test]
    fn parse_states_arg_rejects_a_range_past_the_frame_count() {
        assert!(parse_states_arg("walk:0-99", 10).is_err());
    }

    #[test]
    fn default_state_covers_every_frame() {
        let states = default_state(5);
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].name, "default");
        assert_eq!((states[0].start, states[0].end), (0, 4));
    }

    #[test]
    fn render_manifest_includes_every_state_and_the_native_path() {
        let states = vec![StateSpec {
            name: "walk".to_string(),
            start: 0,
            end: 1,
        }];
        let params = ManifestParams {
            name: "cat_walking",
            sheet_path: "assets/cat_walking/cat_walking_sheet.png",
            native_path: "assets/cat_walking/cat_walking_frames.rgba",
            frame_width: 128,
            frame_height: 72,
            columns: 8,
            rows: 4,
            states: &states,
            fps: 12.0,
        };

        let manifest = render_manifest(&params);

        assert!(manifest.contains("name = \"cat_walking\""));
        assert!(manifest.contains("native_path = \"assets/cat_walking/cat_walking_frames.rgba\""));
        assert!(manifest.contains("walk = {"));
        assert!(manifest.contains("frames = { 0, 1 }"));
        assert!(manifest.contains("initial_state = \"walk\""));
    }
}
