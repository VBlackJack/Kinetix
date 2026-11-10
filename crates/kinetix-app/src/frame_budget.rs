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

//! Per-frame budgets that cap bursts of repeated effects.

/// Spends one unit of `budget`; false once it is exhausted.
pub fn take_budget(budget: &mut u32) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_BUDGET: u32 = 3;

    #[test]
    fn grants_exactly_the_budget_then_refuses() {
        let mut budget = TEST_BUDGET;
        let granted = (0..TEST_BUDGET * 2)
            .filter(|_| take_budget(&mut budget))
            .count();
        assert_eq!(granted, TEST_BUDGET as usize);
        assert_eq!(budget, 0);
    }

    #[test]
    fn an_empty_budget_stays_empty() {
        let mut budget = 0;
        assert!(!take_budget(&mut budget));
        assert_eq!(budget, 0);
    }
}
