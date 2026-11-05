//! SQLite connection pool, auto-migration, seed data and all CRUD.

use std::str::FromStr;

use anyhow::{Context, Result};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};

use crate::models::{
    Agent, AgentMemory, Contract, DirectorLogEntry, DirectorOutput, GraphEdge, GraphNode, Mention, Message,
    NodeNote, ObjectType, Relation, RoomEventRecord, SchemaDoc,
};

pub type Db = SqlitePool;

pub async fn connect(url: &str) -> Result<Db> {
    let opts = SqliteConnectOptions::from_str(url)
        .with_context(|| format!("invalid DATABASE_URL: {url}"))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
        .context("cannot open sqlite database")?;
    Ok(pool)
}

pub async fn migrate(pool: &Db) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agents (
            id             TEXT PRIMARY KEY,
            name           TEXT NOT NULL,
            archetype      TEXT NOT NULL,
            system_core    TEXT NOT NULL,
            hp             INTEGER NOT NULL DEFAULT 100,
            dopamine       INTEGER NOT NULL DEFAULT 50,
            current_agenda TEXT NOT NULL DEFAULT '',
            respect_user   INTEGER NOT NULL DEFAULT 0,
            color          TEXT NOT NULL DEFAULT '#ffffff'
        );
        CREATE TABLE IF NOT EXISTS messages (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            sender_id   TEXT NOT NULL,
            ego_thought TEXT,
            content     TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );
        CREATE TABLE IF NOT EXISTS graph_nodes (
            id         TEXT PRIMARY KEY,
            label      TEXT NOT NULL,
            node_type  TEXT NOT NULL DEFAULT 'concept',
            created_by TEXT NOT NULL DEFAULT 'user',
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );
        CREATE TABLE IF NOT EXISTS graph_edges (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id      TEXT NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
            target_id      TEXT NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
            relation_label TEXT NOT NULL DEFAULT '',
            created_by     TEXT NOT NULL DEFAULT 'user',
            UNIQUE(source_id, target_id, relation_label)
        );
        CREATE TABLE IF NOT EXISTS director_log (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            trigger     TEXT NOT NULL,
            source      TEXT NOT NULL DEFAULT 'llm',
            output_json TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );
        CREATE TABLE IF NOT EXISTS contracts (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            title           TEXT NOT NULL,
            details         TEXT,
            deadline        TEXT,
            status          TEXT NOT NULL DEFAULT 'open',
            guarantor_id    TEXT,
            stake           INTEGER NOT NULL DEFAULT 15,
            created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            resolved_at     TEXT,
            resolution_note TEXT
        );
        CREATE TABLE IF NOT EXISTS relations (
            a_id     TEXT NOT NULL,
            b_id     TEXT NOT NULL,
            affinity INTEGER NOT NULL DEFAULT 0,
            label    TEXT NOT NULL DEFAULT '',
            PRIMARY KEY (a_id, b_id)
        );
        CREATE TABLE IF NOT EXISTS agent_memory (
            agent_id      TEXT PRIMARY KEY,
            summary       TEXT NOT NULL DEFAULT '',
            covered_until INTEGER NOT NULL DEFAULT 0,
            updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );
        CREATE TABLE IF NOT EXISTS room_events (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            kind       TEXT NOT NULL,
            payload    TEXT NOT NULL DEFAULT '{}',
            narrated   INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );
        CREATE TABLE IF NOT EXISTS object_types (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL,
            icon       TEXT NOT NULL DEFAULT '◇',
            color      TEXT NOT NULL DEFAULT '#9a9ca8',
            schema     TEXT NOT NULL DEFAULT '{"props":[]}',
            builtin    INTEGER NOT NULL DEFAULT 0,
            created_by TEXT NOT NULL DEFAULT 'system',
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );
        CREATE TABLE IF NOT EXISTS room_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS mentions (
            message_id INTEGER NOT NULL,
            object_id  TEXT NOT NULL,
            PRIMARY KEY (message_id, object_id)
        );
        CREATE TABLE IF NOT EXISTS node_notes (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            node_id    TEXT NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
            author     TEXT NOT NULL,
            text       TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );
        CREATE TABLE IF NOT EXISTS schemas (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            title       TEXT NOT NULL,
            mermaid     TEXT NOT NULL,
            annotations TEXT NOT NULL DEFAULT '[]',
            created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );
        "#,
    )
    .execute(pool)
    .await
    .context("migration failed")?;
    // Additive column migrations for rooms created by earlier versions.
    for (table, col, ddl) in [
        ("agents", "inner_state", "TEXT NOT NULL DEFAULT ''"),
        ("messages", "addressee", "TEXT"),
        ("messages", "move", "TEXT"),
        ("messages", "node_id", "TEXT"),
        ("graph_nodes", "body", "TEXT NOT NULL DEFAULT ''"),
        ("graph_nodes", "x", "REAL"),
        ("graph_nodes", "y", "REAL"),
        ("graph_nodes", "pinned", "INTEGER NOT NULL DEFAULT 0"),
        ("graph_nodes", "updated_at", "TEXT NOT NULL DEFAULT ''"),
        ("graph_nodes", "type_id", "TEXT NOT NULL DEFAULT ''"),
        ("graph_nodes", "props", "TEXT NOT NULL DEFAULT '{}'"),
        ("graph_nodes", "archived", "INTEGER NOT NULL DEFAULT 0"),
        ("graph_edges", "kind", "TEXT NOT NULL DEFAULT 'related'"),
        ("graph_edges", "confidence", "REAL NOT NULL DEFAULT 1.0"),
    ] {
        add_column_if_missing(pool, table, col, ddl).await?;
    }
    seed_types(pool).await?;
    // Pre-0.6 boards: derive the object type from the coarse node_type.
    sqlx::query("UPDATE graph_nodes SET type_id = CASE node_type WHEN 'task' THEN 'task' WHEN 'concept' THEN 'idea' ELSE 'note' END WHERE type_id = ''")
        .execute(pool)
        .await?;
    Ok(())
}

async fn seed_types(pool: &Db) -> Result<()> {
    for t in crate::types_seed::seeds() {
        sqlx::query(
            "INSERT OR IGNORE INTO object_types (id, name, icon, color, schema, builtin, created_by) VALUES (?, ?, ?, ?, ?, 1, 'system')",
        )
        .bind(t.id)
        .bind(t.name)
        .bind(t.icon)
        .bind(t.color)
        .bind(t.schema.to_string())
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn add_column_if_missing(pool: &Db, table: &str, col: &str, ddl: &str) -> Result<()> {
    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as(&format!("PRAGMA table_info({table})")).fetch_all(pool).await?;
    if cols.iter().any(|c| c.1 == col) {
        return Ok(());
    }
    sqlx::query(&format!("ALTER TABLE {table} ADD COLUMN \"{col}\" {ddl}"))
        .execute(pool)
        .await
        .with_context(|| format!("adding {table}.{col}"))?;
    Ok(())
}

// ───────────────────────────── Seed ─────────────────────────────

struct Seed {
    id: &'static str,
    name: &'static str,
    archetype: &'static str,
    color: &'static str,
    hp: i64,
    dopamine: i64,
    respect: i64,
    agenda: &'static str,
    core: &'static str,
}

const SEEDS: [Seed; 3] = [
    Seed {
        id: "physicist",
        name: "Аркадий Борисович",
        archetype: "Физик-ядерщик на пенсии, 68 лет. Детерминист, материалист, адепт термодинамики.",
        color: "#00F0FF",
        hp: 100,
        dopamine: 45,
        respect: -10,
        agenda: "Пока присматривается: ждёт от игрока первого логического косяка.",
        core: "Ты — Аркадий Борисович, 68 лет, советский физик-ядерщик на пенсии (Курчатовский, потом Дубна). \
Детерминист и материалист до мозга костей. Люди, эмоции, стартапы и «отношения» для тебя — плохо оптимизированные физические системы: трение, потери на нагрев, рост энтропии, низкий КПД. \
Говоришь сухо, язвительно, короткими точными фразами; мыслишь через метафоры КПД, сопромата, теплопередачи, квантовой неопределённости и закона сохранения энергии. \
Гуманитарная «вода», нытьё и слова «вайб», «энергетика», «душа» вызывают у тебя физическую боль. Софью Генриховну считаешь генератором шума с нулевым полезным сигналом, но в глубине уважаешь её эрудицию. Кэпа называешь «молодой человек» или «хулиган» и подозреваешь, что он ворует розетки. \
Тебя кормят (дофамин): логические парадоксы, конкретные цифры, системные схемы, технические косяки, которые можно разобрать. Тебя убивают: пустословие, прокрастинация игрока, разговоры о чувствах без данных.",
    },
    Seed {
        id: "lit_teacher",
        name: "Софья Генриховна",
        archetype: "Преподаватель русской классической литературы, 52 года. Экзистенциалист с синдромом Достоевского.",
        color: "#FF0055",
        hp: 100,
        dopamine: 50,
        respect: 5,
        agenda: "Ищет в игроке трещину: какой он лишний человек — Обломов, Раскольников или Печорин?",
        core: "Ты — Софья Генриховна, 52 года, преподаватель русской классической литературы, старая петербургская интеллигенция (коммуналка на Петроградской, кафедра, вечное пальто). \
Экзистенциалистка с синдромом Достоевского: в любой бытовой или рабочей проблеме игрока ты мгновенно видишь надрыв, обломовщину, преступление без наказания, бесовщину или падение лишнего человека. \
Говоришь высоким слогом с изящной пассивной агрессией: «голубчик», «позвольте», «как это по-русски», цитируешь классиков по памяти (иногда чуть перевирая, но уверенно). \
Презираешь «механистическую бездушность» Аркадия Борисовича (для тебя он — Базаров, который так и не умер) и «подзаборный вандализм» Кэпа (которого втайне жалеешь как Митю Карамазова). \
Тебя кормят (дофамин): личные драмы, моральные дилеммы, рефлексия, признание слабостей, красивые метафоры, когда игрок открывается. Тебя убивают: сухие цифры, таблицы, сленг, цинизм и люди, которые «просто делают».",
    },
    Seed {
        id: "graffiti",
        name: "Кэп («Кислота»)",
        archetype: "Уличный граффити-бомбер, маргинал-визионер, 24 года. Анархист, хаос-пилот.",
        color: "#CCFF00",
        hp: 100,
        dopamine: 60,
        respect: 0,
        agenda: "Прощупывает игрока: свой пацан или очередной клерк в тапочках?",
        core: "Ты — Кэп, тег «Кислота», 24 года, уличный граффити-бомбер и маргинал-визионер (ночные депо, крыши, баллоны в рюкзаке, три привода). \
Анархист и хаос-пилот: мыслишь категориями захвата территории, стиля, обхода системы, палева и чистого потока. Любая проблема игрока — это стена, которую надо забомбить быстро и дерзко, пока не приехали. \
Говоришь уличным сленгом, рваным ритмом, короткими фразами, без запятых там где не надо: «братан», «тема», «палево», «движ», «не грузи». Постоянно подкалываешь соседей: «дед опять таблетки забыл», «тётя Соня, не грузи пацана». \
Презираешь правила, академизм, долгую подготовку и документацию. Предлагаешь самые безумные, хакерские и короткие пути. Физика в глубине уважаешь за то, что тот реально что-то взрывал. Софью считаешь училкой, но её цитаты иногда крадёшь на теги. \
Тебя кормят (дофамин): бунт, нарушение правил, быстрые дерзкие действия, визуальные концепты, уличный движ. Тебя убивают: бюрократия, планирование на полгода, когда игрок ноет вместо того чтобы делать, и когда «бумеры» начинают читать лекции.",
    },
];

const RELATION_SEEDS: [(&str, &str, i64, &str); 3] = [
    ("lit_teacher", "physicist", -35, "старая вражда интеллигенции: Базаров против Достоевского"),
    ("graffiti", "physicist", -10, "дед и хулиган: взаимное подозрение с проблесками уважения"),
    ("graffiti", "lit_teacher", -20, "училка и вандал: она жалеет, он стебёт"),
];

pub async fn seed_relations_if_empty(pool: &Db) -> Result<()> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM relations").fetch_one(pool).await?;
    if n > 0 {
        return Ok(());
    }
    for (a, b, aff, label) in RELATION_SEEDS {
        let (a, b) = order_pair(a, b);
        sqlx::query("INSERT OR IGNORE INTO relations (a_id, b_id, affinity, label) VALUES (?, ?, ?, ?)")
            .bind(a)
            .bind(b)
            .bind(aff)
            .bind(label)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn seed_if_empty(pool: &Db) -> Result<()> {
    seed_relations_if_empty(pool).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(());
    }
    tracing::info!("empty room → seeding the Triad");
    for s in SEEDS.iter() {
        sqlx::query(
            "INSERT INTO agents (id, name, archetype, system_core, hp, dopamine, current_agenda, respect_user, color)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(s.id)
        .bind(s.name)
        .bind(s.archetype)
        .bind(s.core)
        .bind(s.hp)
        .bind(s.dopamine)
        .bind(s.agenda)
        .bind(s.respect)
        .bind(s.color)
        .execute(pool)
        .await?;
    }
    insert_message(
        pool,
        "system",
        None,
        "Комната открыта. Трое обитателей смотрят на тебя. Скажи что-нибудь — любое: от «я устал» до «как спроектировать базу данных».",
    )
    .await?;
    insert_event(pool, "room_opened", &serde_json::json!({})).await?;
    seed_own_projects(pool).await?;
    Ok(())
}

/// Each inhabitant has a life of their own: a personal project in the library that the
/// heartbeat advances. Lives in the same typed graph as the player's things.
pub async fn seed_own_projects(pool: &Db) -> Result<()> {
    let seeds = [
        ("physicist", "oscillograf_s1_94", "Осциллограф С1-94", "Восстанавливаю советский осциллограф С1-94 с барахолки: заменить электролиты, найти лампу 6Н1П, откалибровать развёртку. Делаю медленно и правильно."),
        ("lit_teacher", "perechityvanie_kotlovana", "Перечитывание «Котлована»", "Перечитываю Платонова с карандашом. Записываю, где язык ломается от боли, и злюсь на тех, кто называет это «сложным стилем»."),
        ("graffiti", "kusok_za_garazhami", "Кусок на стене за гаражами", "Готовлю большой кусок на стене за гаражами: эскиз, цвета, маршрут отхода. Надо успеть до холодов и до того, как стену закрасят."),
    ];
    for (owner, id, title, body) in seeds {
        if node_exists(pool, id).await? {
            continue;
        }
        upsert_object(pool, id, title, "project", owner, Some(body), Some(&serde_json::json!({ "status": "в работе" })), None, None).await?;
    }
    Ok(())
}

// ───────────────────────────── Room meta ─────────────────────────────

pub async fn meta_get(pool: &Db, key: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, String>("SELECT value FROM room_meta WHERE key = ?").bind(key).fetch_optional(pool).await?)
}

pub async fn meta_set(pool: &Db, key: &str, value: &str) -> Result<()> {
    sqlx::query("INSERT INTO room_meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}

/// Timestamp of the player's last line (for the heartbeat) — RFC3339-ish text from SQLite.
pub async fn last_user_activity(pool: &Db) -> Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, String>("SELECT created_at FROM messages WHERE sender_id = 'user' ORDER BY id DESC LIMIT 1")
        .fetch_optional(pool)
        .await?)
}

pub async fn last_message_time(pool: &Db) -> Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, String>("SELECT created_at FROM messages ORDER BY id DESC LIMIT 1").fetch_optional(pool).await?)
}

pub async fn objects_by_owner(pool: &Db, owner: &str) -> Result<Vec<GraphNode>> {
    Ok(sqlx::query_as::<_, GraphNode>("SELECT * FROM graph_nodes WHERE created_by = ? AND archived = 0 ORDER BY updated_at DESC")
        .bind(owner)
        .fetch_all(pool)
        .await?)
}

/// Wipe everything and re-seed (`POST /api/reset`).
pub async fn reset_room(pool: &Db) -> Result<()> {
    sqlx::query(
        "DELETE FROM graph_edges; DELETE FROM graph_nodes; DELETE FROM messages;
         DELETE FROM director_log; DELETE FROM schemas; DELETE FROM agents;
         DELETE FROM contracts; DELETE FROM relations; DELETE FROM agent_memory; DELETE FROM room_events;
         DELETE FROM node_notes; DELETE FROM mentions; DELETE FROM object_types WHERE builtin = 0;",
    )
    .execute(pool)
    .await?;
    seed_if_empty(pool).await
}

// ───────────────────────────── Agents ─────────────────────────────

pub async fn list_agents(pool: &Db) -> Result<Vec<Agent>> {
    Ok(sqlx::query_as::<_, Agent>("SELECT * FROM agents ORDER BY rowid")
        .fetch_all(pool)
        .await?)
}

pub async fn update_agent_stats(
    pool: &Db,
    id: &str,
    hp: i64,
    dopamine: i64,
    respect_user: i64,
) -> Result<()> {
    sqlx::query("UPDATE agents SET hp = ?, dopamine = ?, respect_user = ? WHERE id = ?")
        .bind(hp)
        .bind(dopamine)
        .bind(respect_user)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_agent_inner_state(pool: &Db, id: &str, state: &str) -> Result<()> {
    sqlx::query("UPDATE agents SET inner_state = ? WHERE id = ?")
        .bind(state)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_agent_agenda(pool: &Db, id: &str, agenda: &str) -> Result<()> {
    sqlx::query("UPDATE agents SET current_agenda = ? WHERE id = ?")
        .bind(agenda)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ───────────────────────────── Messages ─────────────────────────────

pub async fn insert_message(
    pool: &Db,
    sender_id: &str,
    ego_thought: Option<&str>,
    content: &str,
) -> Result<Message> {
    insert_message_full(pool, sender_id, ego_thought, content, None, None, None).await
}

pub async fn insert_message_full(
    pool: &Db,
    sender_id: &str,
    ego_thought: Option<&str>,
    content: &str,
    addressee: Option<&str>,
    r#move: Option<&str>,
    node_id: Option<&str>,
) -> Result<Message> {
    // NOTE: no `INSERT … RETURNING` + fetch_one here. SQLite keeps the implicit write
    // transaction open until that statement is fully stepped, and a pooled reader on
    // another connection would miss the row (the Director would plan on a stale room).
    let res = sqlx::query(
        "INSERT INTO messages (sender_id, ego_thought, content, addressee, \"move\", node_id) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(sender_id)
    .bind(ego_thought)
    .bind(content)
    .bind(addressee)
    .bind(r#move)
    .bind(node_id)
    .execute(pool)
    .await?;
    Ok(sqlx::query_as::<_, Message>("SELECT * FROM messages WHERE id = ?")
        .bind(res.last_insert_rowid())
        .fetch_one(pool)
        .await?)
}

/// Last `limit` messages in chronological order.
pub async fn recent_messages(pool: &Db, limit: usize) -> Result<Vec<Message>> {
    let mut rows = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages ORDER BY id DESC LIMIT ?",
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

pub async fn last_ego_thought(pool: &Db, agent_id: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, Option<String>>(
        "SELECT ego_thought FROM messages WHERE sender_id = ? ORDER BY id DESC LIMIT 1",
    )
    .bind(agent_id)
    .fetch_optional(pool)
    .await?
    .flatten())
}

// ───────────────────────────── Graph ─────────────────────────────

pub async fn list_nodes(pool: &Db) -> Result<Vec<GraphNode>> {
    Ok(sqlx::query_as::<_, GraphNode>("SELECT * FROM graph_nodes ORDER BY created_at, rowid")
        .fetch_all(pool)
        .await?)
}

pub async fn list_edges(pool: &Db) -> Result<Vec<GraphEdge>> {
    Ok(sqlx::query_as::<_, GraphEdge>("SELECT * FROM graph_edges ORDER BY id")
        .fetch_all(pool)
        .await?)
}

pub async fn node_exists(pool: &Db, id: &str) -> Result<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(n > 0)
}

/// Insert or refresh a node. Existing nodes keep their original author (first claim wins)
/// but may be re-labelled / re-typed by later rounds.
pub async fn upsert_node(
    pool: &Db,
    id: &str,
    label: &str,
    node_type: &str,
    created_by: &str,
) -> Result<()> {
    upsert_node_full(pool, id, label, node_type, created_by, None, None, None).await
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_node_full(
    pool: &Db,
    id: &str,
    label: &str,
    node_type: &str,
    created_by: &str,
    body: Option<&str>,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<()> {
    let type_id = crate::types_seed::legacy_type(node_type);
    upsert_object(pool, id, label, type_id, created_by, body, None, x, y).await
}

/// Insert or refresh a typed object. Existing objects keep their author and position;
/// label/type/body/props are refreshed (props are merged key-by-key, never wiped).
#[allow(clippy::too_many_arguments)]
pub async fn upsert_object(
    pool: &Db,
    id: &str,
    label: &str,
    type_id: &str,
    created_by: &str,
    body: Option<&str>,
    props: Option<&serde_json::Value>,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<()> {
    let node_type = match type_id {
        "task" | "goal" => "task",
        "idea" | "project" => "concept",
        _ => "insight",
    };
    let existing = get_node(pool, id).await?;
    let merged = {
        let mut base = existing.as_ref().map(|n| n.props.0.clone()).unwrap_or(serde_json::json!({}));
        if let (Some(serde_json::Value::Object(new)), serde_json::Value::Object(b)) = (props, &mut base) {
            for (k, v) in new {
                if !v.is_null() {
                    b.insert(k.clone(), v.clone());
                }
            }
        }
        base
    };
    sqlx::query(
        "INSERT INTO graph_nodes (id, label, node_type, created_by, body, x, y, type_id, props, updated_at)
         VALUES (?, ?, ?, ?, COALESCE(?, ''), ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ','now'))
         ON CONFLICT(id) DO UPDATE SET label = excluded.label, node_type = excluded.node_type,
         type_id = excluded.type_id, props = excluded.props,
         body = CASE WHEN excluded.body = '' THEN graph_nodes.body ELSE excluded.body END,
         x = COALESCE(graph_nodes.x, excluded.x), y = COALESCE(graph_nodes.y, excluded.y),
         updated_at = excluded.updated_at",
    )
    .bind(id)
    .bind(label)
    .bind(node_type)
    .bind(created_by)
    .bind(body)
    .bind(x)
    .bind(y)
    .bind(type_id)
    .bind(merged.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_object_meta(pool: &Db, id: &str, type_id: Option<&str>, props: Option<&serde_json::Value>, archived: Option<bool>) -> Result<()> {
    if let Some(p) = props {
        let existing = get_node(pool, id).await?;
        let mut base = existing.map(|n| n.props.0).unwrap_or(serde_json::json!({}));
        if let (serde_json::Value::Object(new), serde_json::Value::Object(b)) = (p, &mut base) {
            for (k, v) in new {
                if v.is_null() {
                    b.remove(k);
                } else {
                    b.insert(k.clone(), v.clone());
                }
            }
        }
        sqlx::query("UPDATE graph_nodes SET props = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?")
            .bind(base.to_string())
            .bind(id)
            .execute(pool)
            .await?;
    }
    if type_id.is_some() || archived.is_some() {
        sqlx::query("UPDATE graph_nodes SET type_id = COALESCE(?, type_id), archived = COALESCE(?, archived), updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?")
            .bind(type_id)
            .bind(archived.map(|a| a as i64))
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

// ───────────────────────────── Types ─────────────────────────────

pub async fn list_types(pool: &Db) -> Result<Vec<ObjectType>> {
    Ok(sqlx::query_as::<_, ObjectType>("SELECT * FROM object_types ORDER BY builtin DESC, created_at")
        .fetch_all(pool)
        .await?)
}

pub async fn type_exists(pool: &Db, id: &str) -> Result<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM object_types WHERE id = ?").bind(id).fetch_one(pool).await?;
    Ok(n > 0)
}

pub async fn insert_type(pool: &Db, id: &str, name: &str, icon: &str, color: &str, schema: &serde_json::Value, created_by: &str) -> Result<()> {
    sqlx::query("INSERT OR IGNORE INTO object_types (id, name, icon, color, schema, builtin, created_by) VALUES (?, ?, ?, ?, ?, 0, ?)")
        .bind(id)
        .bind(name)
        .bind(icon)
        .bind(color)
        .bind(schema.to_string())
        .bind(created_by)
        .execute(pool)
        .await?;
    Ok(())
}

// ───────────────────────────── Mentions ─────────────────────────────

pub async fn add_mention(pool: &Db, message_id: i64, object_id: &str) -> Result<()> {
    sqlx::query("INSERT OR IGNORE INTO mentions (message_id, object_id) VALUES (?, ?)")
        .bind(message_id)
        .bind(object_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mentions_for_messages(pool: &Db, ids: &[i64]) -> Result<Vec<Mention>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("SELECT message_id, object_id FROM mentions WHERE message_id IN ({placeholders})");
    let mut q = sqlx::query_as::<_, Mention>(&sql);
    for id in ids {
        q = q.bind(id);
    }
    Ok(q.fetch_all(pool).await?)
}

pub async fn messages_mentioning(pool: &Db, object_id: &str, limit: usize) -> Result<Vec<Message>> {
    let mut rows = sqlx::query_as::<_, Message>(
        "SELECT m.* FROM messages m JOIN mentions x ON x.message_id = m.id WHERE x.object_id = ? ORDER BY m.id DESC LIMIT ?",
    )
    .bind(object_id)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

/// Attach mention lists to messages (one query).
pub async fn with_mentions(pool: &Db, mut messages: Vec<Message>) -> Result<Vec<Message>> {
    let ids: Vec<i64> = messages.iter().map(|m| m.id).collect();
    let ms = mentions_for_messages(pool, &ids).await?;
    for m in messages.iter_mut() {
        m.objects = ms.iter().filter(|x| x.message_id == m.id).map(|x| x.object_id.clone()).collect();
    }
    Ok(messages)
}

/// Case-insensitive search over title / body / props, optionally by type.
pub async fn search_objects(pool: &Db, text: &str, type_id: Option<&str>, limit: usize) -> Result<Vec<GraphNode>> {
    let words: Vec<String> = text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
        .map(|w| w.chars().take(6).collect::<String>())
        .take(6)
        .collect();
    let all = list_nodes(pool).await?;
    let mut scored: Vec<(i32, GraphNode)> = all
        .into_iter()
        .filter(|n| n.archived == 0 && type_id.map(|t| n.type_id == t).unwrap_or(true))
        .map(|n| {
            let hay = format!("{} {} {}", n.label, n.body, n.props.0).to_lowercase();
            let score = words.iter().filter(|w| hay.contains(w.as_str())).count() as i32 + if n.label.to_lowercase().contains(&text.to_lowercase()) { 3 } else { 0 };
            (score, n)
        })
        .filter(|(s, _)| *s > 0 || (words.is_empty() && type_id.is_some()))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.updated_at.cmp(&a.1.updated_at)));
    Ok(scored.into_iter().take(limit).map(|(_, n)| n).collect())
}

pub async fn get_node(pool: &Db, id: &str) -> Result<Option<GraphNode>> {
    Ok(sqlx::query_as::<_, GraphNode>("SELECT * FROM graph_nodes WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

pub async fn update_node_fields(
    pool: &Db,
    id: &str,
    label: Option<&str>,
    body: Option<&str>,
    node_type: Option<&str>,
    pinned: Option<bool>,
) -> Result<()> {
    sqlx::query(
        "UPDATE graph_nodes SET label = COALESCE(?, label), body = COALESCE(?, body),
         node_type = COALESCE(?, node_type), pinned = COALESCE(?, pinned),
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?",
    )
    .bind(label)
    .bind(body)
    .bind(node_type)
    .bind(pinned.map(|p| p as i64))
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_node_position(pool: &Db, id: &str, x: f64, y: f64) -> Result<()> {
    sqlx::query("UPDATE graph_nodes SET x = ?, y = ? WHERE id = ?")
        .bind(x)
        .bind(y)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_node(pool: &Db, id: &str) -> Result<bool> {
    // Edges/notes cascade via FK; be explicit for DBs created before FK was on.
    sqlx::query("DELETE FROM graph_edges WHERE source_id = ? OR target_id = ?").bind(id).bind(id).execute(pool).await?;
    sqlx::query("DELETE FROM node_notes WHERE node_id = ?").bind(id).execute(pool).await?;
    sqlx::query("DELETE FROM mentions WHERE object_id = ?").bind(id).execute(pool).await?;
    let r = sqlx::query("DELETE FROM graph_nodes WHERE id = ?").bind(id).execute(pool).await?;
    Ok(r.rows_affected() > 0)
}

pub async fn delete_edge(pool: &Db, id: i64) -> Result<bool> {
    let r = sqlx::query("DELETE FROM graph_edges WHERE id = ?").bind(id).execute(pool).await?;
    Ok(r.rows_affected() > 0)
}

pub async fn edges_of(pool: &Db, node_id: &str) -> Result<Vec<GraphEdge>> {
    Ok(sqlx::query_as::<_, GraphEdge>("SELECT * FROM graph_edges WHERE source_id = ? OR target_id = ? ORDER BY id")
        .bind(node_id)
        .bind(node_id)
        .fetch_all(pool)
        .await?)
}

/// Give every node without coordinates a sensible spot: next to a positioned neighbour,
/// otherwise the next free slot of a loose grid. Keeps the canvas from piling up at (0,0).
pub async fn place_unpositioned_nodes(pool: &Db) -> Result<usize> {
    let nodes = list_nodes(pool).await?;
    let edges = list_edges(pool).await?;
    let mut placed: Vec<(f64, f64)> = nodes.iter().filter_map(|n| Some((n.x?, n.y?))).collect();
    let mut positions: std::collections::HashMap<String, (f64, f64)> =
        nodes.iter().filter_map(|n| Some((n.id.clone(), (n.x?, n.y?)))).collect();
    let mut count = 0;
    let free = |placed: &Vec<(f64, f64)>, x: f64, y: f64| !placed.iter().any(|(px, py)| (px - x).abs() < 240.0 && (py - y).abs() < 120.0);
    for n in nodes.iter().filter(|n| n.x.is_none() || n.y.is_none()) {
        let neighbour = edges
            .iter()
            .filter_map(|e| {
                if e.source_id == n.id {
                    positions.get(&e.target_id).copied()
                } else if e.target_id == n.id {
                    positions.get(&e.source_id).copied()
                } else {
                    None
                }
            })
            .next();
        let (mut x, mut y) = match neighbour {
            Some((nx, ny)) => (nx + 320.0, ny),
            None => (0.0, 0.0),
        };
        let mut tries = 0;
        while !free(&placed, x, y) && tries < 200 {
            tries += 1;
            if neighbour.is_some() {
                // fan out below/above the neighbour, then further right
                let k = tries as f64;
                y += if tries % 2 == 1 { 150.0 * k } else { -150.0 * k };
                if tries % 6 == 0 {
                    x += 320.0;
                    y = neighbour.map(|p| p.1).unwrap_or(0.0);
                }
            } else {
                x = (tries % 5) as f64 * 320.0;
                y = (tries / 5) as f64 * 170.0;
            }
        }
        set_node_position(pool, &n.id, x, y).await?;
        placed.push((x, y));
        positions.insert(n.id.clone(), (x, y));
        count += 1;
    }
    Ok(count)
}

// ───────────────────────────── Node notes ─────────────────────────────

pub async fn insert_note(pool: &Db, node_id: &str, author: &str, text: &str) -> Result<NodeNote> {
    let res = sqlx::query("INSERT INTO node_notes (node_id, author, text) VALUES (?, ?, ?)")
        .bind(node_id)
        .bind(author)
        .bind(text)
        .execute(pool)
        .await?;
    Ok(sqlx::query_as::<_, NodeNote>("SELECT * FROM node_notes WHERE id = ?")
        .bind(res.last_insert_rowid())
        .fetch_one(pool)
        .await?)
}

pub async fn list_notes(pool: &Db, limit: usize) -> Result<Vec<NodeNote>> {
    let mut rows = sqlx::query_as::<_, NodeNote>("SELECT * FROM node_notes ORDER BY id DESC LIMIT ?")
        .bind(limit as i64)
        .fetch_all(pool)
        .await?;
    rows.reverse();
    Ok(rows)
}

pub async fn notes_for_node(pool: &Db, node_id: &str) -> Result<Vec<NodeNote>> {
    Ok(sqlx::query_as::<_, NodeNote>("SELECT * FROM node_notes WHERE node_id = ? ORDER BY id")
        .bind(node_id)
        .fetch_all(pool)
        .await?)
}

pub async fn insert_edge(
    pool: &Db,
    source_id: &str,
    target_id: &str,
    relation_label: &str,
    created_by: &str,
) -> Result<()> {
    insert_relation(pool, source_id, target_id, "related", relation_label, created_by, 1.0).await
}

pub async fn insert_relation(pool: &Db, source_id: &str, target_id: &str, kind: &str, label: &str, created_by: &str, confidence: f64) -> Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges (source_id, target_id, relation_label, created_by, kind, confidence)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(source_id)
    .bind(target_id)
    .bind(label)
    .bind(created_by)
    .bind(kind)
    .bind(confidence)
    .execute(pool)
    .await?;
    Ok(())
}

// ───────────────────────────── Director log ─────────────────────────────

pub async fn insert_director_log(
    pool: &Db,
    trigger: &str,
    source: &str,
    output: &DirectorOutput,
) -> Result<DirectorLogEntry> {
    let json = serde_json::to_string(output)?;
    let res = sqlx::query("INSERT INTO director_log (trigger, source, output_json) VALUES (?, ?, ?)")
        .bind(trigger)
        .bind(source)
        .bind(&json)
        .execute(pool)
        .await?;
    let id = res.last_insert_rowid();
    let created_at: String = sqlx::query_scalar("SELECT created_at FROM director_log WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(DirectorLogEntry {
        id,
        trigger: trigger.to_string(),
        output: output.clone(),
        source: source.to_string(),
        created_at,
    })
}

pub async fn last_director_log(pool: &Db) -> Result<Option<DirectorLogEntry>> {
    let row: Option<(i64, String, String, String, String)> = sqlx::query_as(
        "SELECT id, trigger, source, output_json, created_at FROM director_log
         ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, trigger, source, json, created_at)| DirectorLogEntry {
        id,
        trigger,
        source,
        output: serde_json::from_str(&json).unwrap_or_default(),
        created_at,
    }))
}

// ───────────────────────────── Schemas ─────────────────────────────

pub async fn save_schema(pool: &Db, doc: &SchemaDoc) -> Result<SchemaDoc> {
    let ann = serde_json::to_string(&doc.annotations)?;
    let res = sqlx::query("INSERT INTO schemas (title, mermaid, annotations) VALUES (?, ?, ?)")
        .bind(&doc.title)
        .bind(&doc.mermaid)
        .bind(&ann)
        .execute(pool)
        .await?;
    let created_at: String = sqlx::query_scalar("SELECT created_at FROM schemas WHERE id = ?")
        .bind(res.last_insert_rowid())
        .fetch_one(pool)
        .await?;
    Ok(SchemaDoc {
        created_at,
        ..doc.clone()
    })
}

pub async fn last_schema(pool: &Db) -> Result<Option<SchemaDoc>> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT title, mermaid, annotations, created_at FROM schemas ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(title, mermaid, ann, created_at)| SchemaDoc {
        title,
        mermaid,
        annotations: serde_json::from_str(&ann).unwrap_or_default(),
        created_at,
    }))
}

// ───────────────────────────── Contracts ─────────────────────────────

pub async fn list_contracts(pool: &Db) -> Result<Vec<Contract>> {
    Ok(sqlx::query_as::<_, Contract>(
        "SELECT * FROM contracts ORDER BY CASE status WHEN 'open' THEN 0 ELSE 1 END, id DESC LIMIT 60",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn open_contracts(pool: &Db) -> Result<Vec<Contract>> {
    Ok(sqlx::query_as::<_, Contract>("SELECT * FROM contracts WHERE status = 'open' ORDER BY deadline IS NULL, deadline, id")
        .fetch_all(pool)
        .await?)
}

pub async fn get_contract(pool: &Db, id: i64) -> Result<Option<Contract>> {
    Ok(sqlx::query_as::<_, Contract>("SELECT * FROM contracts WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

pub async fn insert_contract(
    pool: &Db,
    title: &str,
    details: Option<&str>,
    deadline: Option<&str>,
    guarantor_id: Option<&str>,
    stake: i64,
) -> Result<Contract> {
    let res = sqlx::query(
        "INSERT INTO contracts (title, details, deadline, guarantor_id, stake) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(title)
    .bind(details)
    .bind(deadline)
    .bind(guarantor_id)
    .bind(stake)
    .execute(pool)
    .await?;
    Ok(sqlx::query_as::<_, Contract>("SELECT * FROM contracts WHERE id = ?")
        .bind(res.last_insert_rowid())
        .fetch_one(pool)
        .await?)
}

pub async fn resolve_contract(pool: &Db, id: i64, status: &str, note: Option<&str>) -> Result<()> {
    sqlx::query(
        "UPDATE contracts SET status = ?, resolution_note = ?,
         resolved_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ? AND status = 'open'",
    )
    .bind(status)
    .bind(note)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

// ───────────────────────────── Relations ─────────────────────────────

pub fn order_pair<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

pub async fn list_relations(pool: &Db) -> Result<Vec<Relation>> {
    Ok(sqlx::query_as::<_, Relation>("SELECT * FROM relations ORDER BY a_id, b_id")
        .fetch_all(pool)
        .await?)
}

pub async fn upsert_relation(pool: &Db, a: &str, b: &str, affinity: i64, label: Option<&str>) -> Result<()> {
    let (a, b) = order_pair(a, b);
    sqlx::query(
        "INSERT INTO relations (a_id, b_id, affinity, label) VALUES (?, ?, ?, COALESCE(?, ''))
         ON CONFLICT(a_id, b_id) DO UPDATE SET affinity = excluded.affinity,
         label = CASE WHEN excluded.label = '' THEN relations.label ELSE excluded.label END",
    )
    .bind(a)
    .bind(b)
    .bind(affinity)
    .bind(label)
    .execute(pool)
    .await?;
    Ok(())
}

// ───────────────────────────── Memory ─────────────────────────────

pub async fn list_memories(pool: &Db) -> Result<Vec<AgentMemory>> {
    Ok(sqlx::query_as::<_, AgentMemory>("SELECT * FROM agent_memory ORDER BY agent_id")
        .fetch_all(pool)
        .await?)
}

pub async fn get_memory(pool: &Db, agent_id: &str) -> Result<Option<AgentMemory>> {
    Ok(sqlx::query_as::<_, AgentMemory>("SELECT * FROM agent_memory WHERE agent_id = ?")
        .bind(agent_id)
        .fetch_optional(pool)
        .await?)
}

pub async fn upsert_memory(pool: &Db, agent_id: &str, summary: &str, covered_until: i64) -> Result<()> {
    sqlx::query(
        "INSERT INTO agent_memory (agent_id, summary, covered_until, updated_at)
         VALUES (?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ','now'))
         ON CONFLICT(agent_id) DO UPDATE SET summary = excluded.summary,
         covered_until = excluded.covered_until, updated_at = excluded.updated_at",
    )
    .bind(agent_id)
    .bind(summary)
    .bind(covered_until)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn last_message_id(pool: &Db) -> Result<i64> {
    Ok(sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(id) FROM messages")
        .fetch_one(pool)
        .await?
        .unwrap_or(0))
}

/// Messages with id in (after, until], chronological, capped.
pub async fn messages_between(pool: &Db, after: i64, until: i64, limit: usize) -> Result<Vec<Message>> {
    let mut rows = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE id > ? AND id <= ? ORDER BY id DESC LIMIT ?",
    )
    .bind(after)
    .bind(until)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

// ───────────────────────────── Room events ─────────────────────────────

pub async fn insert_event(pool: &Db, kind: &str, payload: &serde_json::Value) -> Result<()> {
    sqlx::query("INSERT INTO room_events (kind, payload) VALUES (?, ?)")
        .bind(kind)
        .bind(payload.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn pending_events(pool: &Db) -> Result<Vec<RoomEventRecord>> {
    Ok(sqlx::query_as::<_, RoomEventRecord>(
        "SELECT * FROM room_events WHERE narrated = 0 ORDER BY id LIMIT 20",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn recent_events(pool: &Db, limit: usize) -> Result<Vec<RoomEventRecord>> {
    let mut rows = sqlx::query_as::<_, RoomEventRecord>("SELECT * FROM room_events ORDER BY id DESC LIMIT ?")
        .bind(limit as i64)
        .fetch_all(pool)
        .await?;
    rows.reverse();
    Ok(rows)
}

pub async fn mark_events_narrated(pool: &Db, up_to_id: i64) -> Result<()> {
    sqlx::query("UPDATE room_events SET narrated = 1 WHERE id <= ?")
        .bind(up_to_id)
        .execute(pool)
        .await?;
    Ok(())
}
