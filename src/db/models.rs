use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Course {
    pub id: String,
    pub start: String,
    pub end: String,
    pub duration: i64,
    pub summary: String,
    pub course_name: String,
    #[serde(default)]
    pub course_code: String,
    #[serde(default = "default_activity")]
    pub activity_type: String,
    #[serde(default)]
    pub activity_name: String,
    #[serde(default)]
    pub is_exam: bool,
    #[serde(default)]
    pub is_cancelled: bool,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub teachers: Vec<String>,
    #[serde(default)]
    pub rooms: Vec<String>,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub searchable_text: String,
}

fn default_activity() -> String {
    "AUTRE".to_string()
}

fn default_status() -> String {
    "PLANIFIE".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Promotion {
    pub id: String,
    pub code: String,
    pub name: String,
    pub cycle: String,
    pub year: i32,
    pub semesters: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    pub total_courses: i64,
    pub total_teachers: i64,
    pub total_rooms: i64,
    pub total_groups: i64,
    pub total_promotions: i64,
    pub total_patterns: i64,
    pub recurring_patterns: i64,
    pub min_date: Option<String>,
    pub max_date: Option<String>,
    pub types_breakdown: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeRoomItem {
    pub room_name: String,
    pub is_free: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_course: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub courses_count_today: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_course: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_overlapping: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeRoomsResponse {
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    pub total_rooms: usize,
    pub free_count: usize,
    pub occupied_count: usize,
    pub free_rooms: Vec<FreeRoomItem>,
    pub occupied_rooms: Vec<FreeRoomItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupTreeNode {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub expanded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patterns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<GroupTreeNode>>,
}
