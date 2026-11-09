/*
 * Copyright 2026 Julien Bombled
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Where the board being played comes from, and which rules and persistence follow from it.

use kinetix_core::physics::PackRules;

/// Origin of the current play session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaySource {
    /// The main campaign: the only source that may change saved progress.
    Campaign,
    /// The satisfaction pack: no progress, no sudden death.
    Satisfaction,
    /// A library pack, identified by its id.
    Library(u32),
    /// A board launched from the level editor.
    EditorTest,
}

impl PlaySource {
    /// Engine rules for boards loaded from this source.
    pub fn pack_rules(self) -> PackRules {
        match self {
            Self::Campaign => PackRules::CAMPAIGN,
            Self::Satisfaction => PackRules::non_campaign(false),
            Self::Library(_) | Self::EditorTest => PackRules::non_campaign(true),
        }
    }
}

/// True when saved progress must not be written: a dev-tainted session, or any
/// source other than the campaign.
pub fn progress_frozen(source: PlaySource, dev_tainted: bool) -> bool {
    dev_tainted || source != PlaySource::Campaign
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaign_uses_campaign_rules() {
        assert_eq!(PlaySource::Campaign.pack_rules(), PackRules::CAMPAIGN);
    }

    #[test]
    fn satisfaction_has_no_sudden_death_and_no_campaign() {
        let rules = PlaySource::Satisfaction.pack_rules();
        assert!(!rules.campaign);
        assert!(!rules.sudden_death);
    }

    #[test]
    fn library_and_editor_keep_sudden_death_without_campaign() {
        for source in [PlaySource::Library(7), PlaySource::EditorTest] {
            let rules = source.pack_rules();
            assert!(!rules.campaign);
            assert!(rules.sudden_death);
        }
    }

    #[test]
    fn only_a_clean_campaign_session_saves_progress() {
        assert!(!progress_frozen(PlaySource::Campaign, false));
        assert!(progress_frozen(PlaySource::Campaign, true));
        for source in [
            PlaySource::Satisfaction,
            PlaySource::Library(1),
            PlaySource::EditorTest,
        ] {
            assert!(progress_frozen(source, false));
            assert!(progress_frozen(source, true));
        }
    }
}
