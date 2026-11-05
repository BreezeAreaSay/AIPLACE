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

fn rating(key: &str, label: &str) -> Value {
    json!({ "key": key, "label": label, "kind": "rating", "max": 5 })
}
fn tags() -> Value {
    json!({ "key": "tags", "label": "Теги", "kind": "tags" })
}
fn summary() -> Value {
    json!({ "key": "summary", "label": "Смысл", "kind": "longtext" })
}
fn multi(key: &str, label: &str, options: &[&str]) -> Value {
    json!({ "key": key, "label": label, "kind": "multiselect", "options": options })
}

pub fn seeds() -> Vec<TypeSeed> {
    vec![
        TypeSeed { id: "page", name: "Страница", icon: "📄", color: "#9a9ca8", schema: json!({ "props": [summary(), tags()] }) },
        TypeSeed { id: "note", name: "Заметка", icon: "📝", color: "#c9c9d1", schema: json!({ "props": [summary(), tags()] }) },
        TypeSeed {
            id: "task",
            name: "Задача",
            icon: "☐",
            color: "#CCFF00",
            schema: json!({ "props": [
                summary(),
                p("done", "Готово", "checkbox"),
                p("due", "Срок", "date"),
                sel("priority", "Приоритет", &["низкий", "средний", "высокий"]),
                sel("status", "Статус", &["не начата", "в работе", "заблокирована", "сделана"]),
                json!({ "key": "assignee", "label": "Исполнитель", "kind": "relation", "to": "human" }),
                json!({ "key": "project", "label": "Проект", "kind": "relation", "to": "project" }),
                p("estimate", "Оценка, ч", "number"),
                tags()
            ] }),
        },
        TypeSeed {
            id: "project",
            name: "Проект",
            icon: "◆",
            color: "#00F0FF",
            schema: json!({ "props": [summary(), sel("status", "Статус", &["идея", "в работе", "пауза", "готово"]), p("due", "Срок", "date"), p("progress", "Прогресс %", "number"), p("repo", "GitHub репо", "text"), p("url", "Ссылка", "url"), tags()],
                "template": "## Зачем\n\n## Следующий шаг\n\n- [ ] \n\n## Лог\n" }),
        },
        TypeSeed {
            id: "human",
            name: "Человек",
            icon: "◯",
            color: "#FF0055",
            schema: json!({ "props": [summary(), p("role", "Роль", "text"), p("company", "Компания", "text"), p("relation_to_me", "Кто мне", "text"), p("email", "Email", "email"), p("phone", "Телефон", "phone"), p("birthday", "День рождения", "date"), p("contact", "Контакт", "text"), rating("closeness", "Близость"), tags()] }),
        },
        TypeSeed {
            id: "bookmark",
            name: "Закладка",
            icon: "🔗",
            color: "#7dd3fc",
            schema: json!({ "props": [p("url", "URL", "url"), p("source", "Источник", "text"), p("summary", "Выжимка", "longtext"), p("read", "Прочитано", "checkbox"), rating("rating", "Оценка"), tags()] }),
        },
        TypeSeed {
            id: "idea",
            name: "Идея",
            icon: "✦",
            color: "#f0abfc",
            schema: json!({ "props": [summary(), sel("status", "Статус", &["сырая", "зреет", "проверена", "отброшена"]), rating("excitement", "Горит"), tags()] }),
        },
        TypeSeed {
            id: "goal",
            name: "Цель",
            icon: "◎",
            color: "#fbbf24",
            schema: json!({ "props": [summary(), p("progress", "Прогресс %", "number"), p("due", "Срок", "date"), sel("status", "Статус", &["поставлена", "в работе", "достигнута", "брошена"]), multi("areas", "Сферы", &["здоровье", "деньги", "работа", "отношения", "творчество", "ум", "дом"]), tags()] }),
        },
        TypeSeed {
            id: "book",
            name: "Книга",
            icon: "📖",
            color: "#c084fc",
            schema: json!({ "props": [summary(), p("author", "Автор", "text"), p("year", "Год", "number"), p("pages", "Страниц", "number"), sel("status", "Статус", &["хочу", "читаю", "прочитал", "бросил"]), rating("rating", "Оценка"), p("cover_url", "Обложка", "image"), p("isbn", "ISBN", "text"), p("url", "Ссылка", "url"), tags()],
                "template": "## О чём\n\n## Цитаты\n\n## Что взял\n" }),
        },
        TypeSeed {
            id: "movie",
            name: "Фильм",
            icon: "🎬",
            color: "#fb7185",
            schema: json!({ "props": [summary(), p("director", "Режиссёр", "text"), p("year", "Год", "number"), sel("status", "Статус", &["хочу", "посмотрел", "бросил"]), rating("rating", "Оценка"), p("poster_url", "Постер", "image"), p("runtime", "Минут", "number"), p("url", "Ссылка", "url"), tags()] }),
        },
        TypeSeed {
            id: "recipe",
            name: "Рецепт",
            icon: "🍳",
            color: "#fdba74",
            schema: json!({ "props": [summary(), p("time", "Время", "text"), p("servings", "Порций", "number"), rating("rating", "Оценка"), multi("diet", "Диета", &["веган", "вегетарианское", "без глютена", "без лактозы", "острое"]), p("url", "Источник", "url"), tags()],
                "template": "## Ингредиенты\n\n- \n\n## Шаги\n\n1. \n" }),
        },
        TypeSeed {
            id: "diary",
            name: "Запись дневника",
            icon: "📓",
            color: "#a3e635",
            schema: json!({ "props": [summary(), p("date", "Дата", "date"), rating("mood", "Настроение"), rating("energy", "Энергия"), tags()],
                "template": "## Что было\n\n## Что понял\n\n## Завтра\n" }),
        },
        TypeSeed {
            id: "quote",
            name: "Цитата",
            icon: "❝",
            color: "#fde68a",
            schema: json!({ "props": [summary(), p("author", "Автор", "text"), json!({ "key": "source", "label": "Источник", "kind": "relation" }), rating("rating", "Оценка"), tags()] }),
        },
        TypeSeed {
            id: "event",
            name: "Событие",
            icon: "📅",
            color: "#67e8f9",
            schema: json!({ "props": [summary(), p("date", "Когда", "datetime"), p("place", "Где", "text"), p("url", "Ссылка", "url"), json!({ "key": "with", "label": "С кем", "kind": "relation", "to": "human" }), tags()] }),
        },
        TypeSeed {
            id: "place",
            name: "Место",
            icon: "📍",
            color: "#86efac",
            schema: json!({ "props": [summary(), p("address", "Адрес", "text"), sel("kind", "Что это", &["кафе", "бар", "парк", "магазин", "музей", "город", "другое"]), rating("rating", "Оценка"), p("visited", "Был", "checkbox"), p("url", "Карта", "url"), tags()] }),
        },
        TypeSeed {
            id: "file",
            name: "Файл",
            icon: "📎",
            color: "#d4d4d8",
            schema: json!({ "props": [p("kind", "Вид", "text"), p("summary", "О чём", "longtext"), tags()] }),
        },
        TypeSeed {
            id: "image",
            name: "Картинка",
            icon: "🖼",
            color: "#f9a8d4",
            schema: json!({ "props": [summary(), p("caption", "Подпись", "text"), p("taken", "Когда", "date"), rating("rating", "Оценка"), tags()] }),
        },
        TypeSeed {
            id: "feed",
            name: "Лента",
            icon: "📡",
            color: "#5eead4",
            schema: json!({ "props": [summary(), p("url", "RSS/Atom URL", "url"), p("topics", "Темы", "text"), p("owner", "Кто читает (id персонажа)", "text"), p("active", "Активна", "checkbox"), p("last_fetch", "Последний опрос", "text")] }),
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
