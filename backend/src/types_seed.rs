//! Built-in object types (Anytype-style) with their property schemas. The Archivist may add
//! more at runtime; those are stored with `builtin = 0`.

use serde_json::{json, Value};

pub struct TypeSeed {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub color: &'static str,
    pub schema: Value,
}

fn p(key: &str, label: &str, kind: &str) -> Value {
    json!({ "key": key, "label": label, "kind": kind })
}
fn sel(key: &str, label: &str, options: &[&str]) -> Value {
    json!({ "key": key, "label": label, "kind": "select", "options": options })
}

pub fn seeds() -> Vec<TypeSeed> {
    vec![
        TypeSeed { id: "page", name: "Страница", icon: "📄", color: "#9a9ca8", schema: json!({ "props": [] }) },
        TypeSeed { id: "note", name: "Заметка", icon: "📝", color: "#c9c9d1", schema: json!({ "props": [] }) },
        TypeSeed {
            id: "task",
            name: "Задача",
            icon: "☐",
            color: "#CCFF00",
            schema: json!({ "props": [
                p("done", "Готово", "checkbox"),
                p("due", "Срок", "date"),
                sel("priority", "Приоритет", &["низкий", "средний", "высокий"]),
                sel("status", "Статус", &["не начата", "в работе", "заблокирована", "сделана"]),
                json!({ "key": "assignee", "label": "Исполнитель", "kind": "relation", "to": "human" })
            ] }),
        },
        TypeSeed {
            id: "project",
            name: "Проект",
            icon: "◆",
            color: "#00F0FF",
            schema: json!({ "props": [sel("status", "Статус", &["идея", "в работе", "пауза", "готово"]), p("due", "Срок", "date")] }),
        },
        TypeSeed {
            id: "human",
            name: "Человек",
            icon: "◯",
            color: "#FF0055",
            schema: json!({ "props": [p("role", "Роль", "text"), p("company", "Компания", "text"), p("contact", "Контакт", "text"), p("relation_to_me", "Кто мне", "text")] }),
        },
        TypeSeed {
            id: "bookmark",
            name: "Закладка",
            icon: "🔗",
            color: "#7dd3fc",
            schema: json!({ "props": [p("url", "URL", "url"), p("source", "Источник", "text"), p("summary", "Выжимка", "text")] }),
        },
        TypeSeed {
            id: "idea",
            name: "Идея",
            icon: "✦",
            color: "#f0abfc",
            schema: json!({ "props": [sel("status", "Статус", &["сырая", "зреет", "проверена", "отброшена"])] }),
        },
        TypeSeed {
            id: "goal",
            name: "Цель",
            icon: "◎",
            color: "#fbbf24",
            schema: json!({ "props": [p("progress", "Прогресс %", "number"), p("due", "Срок", "date"), sel("status", "Статус", &["поставлена", "в работе", "достигнута", "брошена"])] }),
        },
        TypeSeed {
            id: "book",
            name: "Книга",
            icon: "📖",
            color: "#c084fc",
            schema: json!({ "props": [p("author", "Автор", "text"), sel("status", "Статус", &["хочу", "читаю", "прочитал"]), p("rating", "Оценка", "number")] }),
        },
        TypeSeed {
            id: "movie",
            name: "Фильм",
            icon: "🎬",
            color: "#fb7185",
            schema: json!({ "props": [p("director", "Режиссёр", "text"), p("year", "Год", "number"), sel("status", "Статус", &["хочу", "посмотрел"]), p("rating", "Оценка", "number")] }),
        },
        TypeSeed {
            id: "recipe",
            name: "Рецепт",
            icon: "🍳",
            color: "#fdba74",
            schema: json!({ "props": [p("ingredients", "Ингредиенты", "text"), p("time", "Время", "text")] }),
        },
        TypeSeed {
            id: "diary",
            name: "Запись дневника",
            icon: "📓",
            color: "#a3e635",
            schema: json!({ "props": [p("date", "Дата", "date"), p("mood", "Настроение", "text")] }),
        },
    ]
}

/// Map the legacy `node_type` of pre-0.6 boards onto a built-in type.
pub fn legacy_type(node_type: &str) -> &'static str {
    match node_type {
        "task" => "task",
        "concept" => "idea",
        _ => "note",
    }
}
