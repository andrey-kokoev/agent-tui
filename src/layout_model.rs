#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTuiLayout {
    pub transcript: Rect,
    pub status: Rect,
    pub composer: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutConfig {
    pub min_transcript_height: u16,
    pub status_height: u16,
    pub composer_height: u16,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            min_transcript_height: 1,
            status_height: 1,
            composer_height: 4,
        }
    }
}

pub fn compute_layout(size: TerminalSize, config: LayoutConfig) -> AgentTuiLayout {
    let width = size.width;
    let status_height = config.status_height.min(size.height);
    let composer_height = config
        .composer_height
        .min(size.height.saturating_sub(status_height));
    let remaining = size.height.saturating_sub(status_height + composer_height);
    let transcript_height = if remaining == 0 {
        0
    } else {
        remaining.max(config.min_transcript_height.min(remaining))
    };

    let transcript = Rect {
        x: 0,
        y: 0,
        width,
        height: transcript_height,
    };
    let status = Rect {
        x: 0,
        y: transcript.height,
        width,
        height: status_height,
    };
    let composer = Rect {
        x: 0,
        y: transcript.height + status.height,
        width,
        height: composer_height,
    };

    AgentTuiLayout {
        transcript,
        status,
        composer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_transcript_status_and_composer_top_to_bottom() {
        let layout = compute_layout(
            TerminalSize {
                width: 120,
                height: 40,
            },
            LayoutConfig::default(),
        );

        assert_eq!(
            layout.transcript,
            Rect {
                x: 0,
                y: 0,
                width: 120,
                height: 35,
            }
        );
        assert_eq!(
            layout.status,
            Rect {
                x: 0,
                y: 35,
                width: 120,
                height: 1,
            }
        );
        assert_eq!(
            layout.composer,
            Rect {
                x: 0,
                y: 36,
                width: 120,
                height: 4,
            }
        );
    }

    #[test]
    fn preserves_total_height_for_short_terminal() {
        let layout = compute_layout(
            TerminalSize {
                width: 80,
                height: 3,
            },
            LayoutConfig::default(),
        );

        assert_eq!(layout.transcript.height, 0);
        assert_eq!(layout.status.height, 1);
        assert_eq!(layout.composer.height, 2);
        assert_eq!(layout.composer.y + layout.composer.height, 3);
    }

    #[test]
    fn supports_zero_height_terminal() {
        let layout = compute_layout(
            TerminalSize {
                width: 80,
                height: 0,
            },
            LayoutConfig::default(),
        );

        assert_eq!(layout.transcript.height, 0);
        assert_eq!(layout.status.height, 0);
        assert_eq!(layout.composer.height, 0);
    }

    #[test]
    fn honors_custom_status_and_composer_heights() {
        let layout = compute_layout(
            TerminalSize {
                width: 100,
                height: 20,
            },
            LayoutConfig {
                min_transcript_height: 1,
                status_height: 2,
                composer_height: 5,
            },
        );

        assert_eq!(layout.transcript.height, 13);
        assert_eq!(layout.status.y, 13);
        assert_eq!(layout.status.height, 2);
        assert_eq!(layout.composer.y, 15);
        assert_eq!(layout.composer.height, 5);
    }
}
