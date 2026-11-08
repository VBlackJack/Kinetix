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

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    En,
    Fr,
}

impl Language {
    pub fn next(self) -> Self {
        match self {
            Self::En => Self::Fr,
            Self::Fr => Self::En,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "EN",
            Self::Fr => "FR",
        }
    }
}

pub struct I18nCatalog {
    entries: HashMap<String, String>,
}

impl I18nCatalog {
    pub fn from_json_str(raw: &str) -> Result<Self, serde_json::Error> {
        let root: Value = serde_json::from_str(raw)?;
        let mut entries = HashMap::new();
        Self::flatten("", &root, &mut entries);
        Ok(Self { entries })
    }

    fn flatten(prefix: &str, val: &Value, out: &mut HashMap<String, String>) {
        if let Value::Object(map) = val {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                match v {
                    Value::String(s) => {
                        out.insert(key, s.clone());
                    }
                    Value::Object(_) => Self::flatten(&key, v, out),
                    _ => {}
                }
            }
        }
    }

    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        self.entries.get(key).map(|s| s.as_str()).unwrap_or(key)
    }
}
