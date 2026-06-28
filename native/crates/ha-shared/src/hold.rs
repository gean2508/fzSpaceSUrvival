//! Hold / cooldown tick logic — shared with TS `sim/hold.ts` for pause/resume tests.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoldProgress {
    pub active: bool,
    pub progress_ms: u32,
    pub duration_ms: u32,
    pub cancelled: bool,
    pub completed: bool,
    pub placing_machine: bool,
}

pub fn advance_hold_progress(hold: HoldProgress, delta_ms: u32, target_valid: bool) -> HoldProgress {
    if hold.completed || hold.cancelled {
        return HoldProgress {
            active: false,
            ..hold
        };
    }
    if !hold.active {
        return hold;
    }
    if !target_valid && !hold.placing_machine {
        return HoldProgress {
            active: false,
            progress_ms: 0,
            ..hold
        };
    }
    let progress_ms = hold.progress_ms.saturating_add(delta_ms);
    if progress_ms >= hold.duration_ms {
        HoldProgress {
            active: false,
            completed: true,
            progress_ms,
            ..hold
        }
    } else {
        HoldProgress {
            progress_ms,
            ..hold
        }
    }
}

pub fn tick_ability_cooldown(remaining_ms: f32, delta_ms: f32) -> f32 {
    if remaining_ms <= 0.0 || delta_ms <= 0.0 {
        return remaining_ms.max(0.0);
    }
    (remaining_ms - delta_ms).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh(duration_ms: u32, placing_machine: bool) -> HoldProgress {
        HoldProgress {
            active: true,
            progress_ms: 0,
            duration_ms,
            cancelled: false,
            completed: false,
            placing_machine,
        }
    }

    #[test]
    fn pause_zero_delta_preserves_hold() {
        let mut h = fresh(2000, true);
        h = advance_hold_progress(h, 500, true);
        assert_eq!(h.progress_ms, 500);
        h = advance_hold_progress(h, 0, true);
        assert_eq!(h.progress_ms, 500);
        h = advance_hold_progress(h, 1500, true);
        assert!(h.completed);
    }

    #[test]
    fn lost_target_resets_mining_hold() {
        let mut h = fresh(800, false);
        h = advance_hold_progress(h, 400, true);
        h = advance_hold_progress(h, 50, false);
        assert!(!h.active);
        assert_eq!(h.progress_ms, 0);
    }

    #[test]
    fn cooldown_pauses_with_zero_delta() {
        let mut cd = 650.0_f32;
        cd = tick_ability_cooldown(cd, 200.0);
        assert_eq!(cd, 450.0);
        cd = tick_ability_cooldown(cd, 0.0);
        assert_eq!(cd, 450.0);
    }
}
