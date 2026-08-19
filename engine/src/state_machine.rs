//! What makes an entity change state: an editor event, or an action request.
//!
//! Split from `ecs.rs`, which held this next to the per-frame step. The two run
//! on different clocks: the step runs every frame and reads only the entity,
//! while these run when Neovim reports something happened and have to decide
//! which entities it applies to.

use crate::ecs::{EventContext, World};

impl World {
    pub fn trigger_action(
        &mut self,
        id_opt: Option<usize>,
        asset_name_opt: Option<&str>,
        action_name: &str,
    ) -> Result<Vec<(usize, String, String)>, String> {
        let mut triggered = Vec::new();

        for entity in &mut self.entities {
            if let Some(target_id) = id_opt {
                if entity.id != target_id {
                    continue;
                }
            } else if let Some(target_asset) = asset_name_opt {
                if entity.asset_name != target_asset {
                    continue;
                }
            }

            if let Some(asset) = self.asset_manager.get(&entity.asset_name) {
                if let Some(action_def) = asset.manifest.custom_actions.get(action_name) {
                    let target_state = action_def.target_state.clone();
                    let duration_s = action_def.duration_ms.map(|ms| ms as f32 / 1000.0);
                    let return_state = action_def.return_state.clone();
                    let is_locked = action_def.is_locked.unwrap_or(true);

                    // Save takeoff position as ground elevation
                    entity.ground_y = entity.y;
                    entity.set_action(target_state.clone(), duration_s, return_state, is_locked);

                    // Apply jump impulse if configured
                    if let Some(state_def) = asset.manifest.states.get(&target_state) {
                        if let Some(impulse) = state_def.physics.jump_impulse_y {
                            entity.vy = impulse;
                        }
                    }

                    triggered.push((entity.id, entity.asset_name.clone(), target_state));
                }
            }
        }

        if triggered.is_empty() {
            Err(format!(
                "Action '{}' not found or matched no active entities",
                action_name
            ))
        } else {
            Ok(triggered)
        }
    }

    pub fn handle_editor_event(&mut self, event_name: &str, context: EventContext) {
        if let Some(col) = context.cursor_col {
            self.focus_x = Some(col * self.cell_w);
        }
        if let Some(row) = context.cursor_row {
            self.focus_y = Some(row * self.cell_h);
        }
        let focus_x = self.focus_x;

        for entity in &mut self.entities {
            if entity.is_locked {
                continue;
            }
            if let Some(asset) = self.asset_manager.get(&entity.asset_name) {
                if let Some(state_def) = asset.manifest.states.get(&entity.current_state) {
                    if let Some(next_state) = state_def.transitions.on_event.get(event_name) {
                        let changed = entity.current_state != *next_state;
                        entity.set_state(next_state.clone());

                        // Orient toward the cursor when picking up a new
                        // behaviour, so the entity looks like it noticed.
                        if changed {
                            if let Some(fx) = focus_x {
                                let moves = asset
                                    .manifest
                                    .states
                                    .get(next_state)
                                    .map(|s| s.physics.target_vx.abs() > 0.0)
                                    .unwrap_or(false);
                                if moves {
                                    entity.face_toward(fx);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
