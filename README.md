# Triad Canvas — киберпсихологическая мультиагентная комната

Гибрид тамагочи, живой графовой базы знаний и личного аналитического совета.
Три радикально разных ИИ-субъекта с HP, дофамином, уважением к игроку и **скрытыми целями, которые на лету
выдумывает LLM-Режиссёр** — никаких захардкоженных повесток в коде.

| Агент | id | Профиль | Цвет узлов |
|---|---|---|---|
| Аркадий Борисович | `physicist` | физик-ядерщик на пенсии, детерминист, энтропия и КПД | `#00F0FF` |
| Софья Генриховна | `lit_teacher` | преподаватель русской классики, достоевщина, обломовщина | `#FF0055` |
| Кэп («Кислота») | `graffiti` | граффити-бомбер, анархист, взлом правил | `#CCFF00` |

## Dual-Target архитектура (локально сегодня → Telegram Mini Apps завтра)

```
frontend/  (Vite + React + TS + Tailwind + Mermaid)   ──HTTP /api/* + SSE──▶   backend/  (Rust + Axum + SQLite)
            mobile-first «стакан» max-w-md                                     ├─ director.rs   LLM-Режиссёр (JSON)
            + десктопный инспектор                                             ├─ agent_runner  prefill <ego_thought>
                                                                               ├─ mermaid.rs    живая блок-схема
                                                                               └─ openrouter.rs OpenRouter client
```

* Фронтенд **никогда** не трогает OS-API: только `fetch('/api/…')` и `EventSource('/api/events')`.
  Тот же бандл открывается в браузере, в Tauri-обёртке и в WebView Telegram.
* Бэкенд — один бинарник. На ноутбуке слушает `localhost:3000`; на VPS за $5 — тот же бинарник, тот же SQLite-файл,
  плюс он сам отдаёт собранный `frontend/dist`. Telegram Mini App просто указывает на его URL.

## Запуск за 3 минуты

```bash
cd backend
cp .env.example .env          # вставь OPENROUTER_API_KEY (без ключа — MOCK-режим с шаблонными репликами)
cargo run
```

```bash
cd frontend
npm install
npm run dev                   # http://localhost:5173 — /api проксируется на :3000
```

Либо один процесс: `cd frontend && npm run build`, затем `cd backend && cargo run` и открывай `http://localhost:3000`.

### Переменные окружения (`backend/.env`)

| Переменная | По умолчанию | Зачем |
|---|---|---|
| `OPENROUTER_API_KEY` | — | Пусто → mock-режим (пайплайн работает, химии нет) |
| `OPENROUTER_MODEL` | `deepseek/deepseek-chat` | Модель агентов. Нужна поддержка assistant-prefill (deepseek / anthropic / gemini / llama) |
| `OPENROUTER_DIRECTOR_MODEL` | = `OPENROUTER_MODEL` | Быстрая JSON-модель для Режиссёра, напр. `google/gemini-2.5-flash` |
| `DATABASE_URL` | `sqlite://triad.db?mode=rwc` | Автомиграция и сид триады при первом старте |
| `STATIC_DIR` | `../frontend/dist` | Если есть `index.html` — бинарник отдаёт SPA |
| `CONTEXT_MESSAGES` | `14` | Сколько реплик видят Режиссёр и агенты |
| `TICK_ENTROPY_HP` / `TICK_ENTROPY_DOPAMINE` | `2` / `3` | Сколько теряет каждый за автономный тик |

## Пайплайн хода («мозг»)

1. **`POST /api/chat`** сохраняет реплику игрока →
2. **`director.rs`** собирает контекст (агенты + статы + текущие повестки, граф, последние N сообщений) и зовёт
   LLM в JSON-режиме. Ответ (`next_speakers`, `updated_agendas`, `stat_deltas`, `graph_mutations`) парсится
   устойчиво: ```json-обёртки, мусор вокруг, висячие запятые. Если модель не ответила — fallback-план
   (случайный говорящий, повестки с прошлого хода), комната не падает.
3. Повестки записываются в `agents.current_agenda`, дельты клампятся (HP 0..100, уважение −100..100),
   узлы/рёбра санитизируются (`mermaid::safe_id`, `sanitize_label`) и попадают в SQLite.
4. **`agent_runner.rs`** для каждого выбранного агента строит динамический анти-ассистентский system prompt
   (состояние, скрытая цель, соседи, узлы доски, предыдущая скрытая мысль) и отправляет запрос с
   **prefill `{"role":"assistant","content":"<ego_thought>"}`**. Ответ режется на `ego_thought` и `public_speech`.
   Агенты ходят последовательно — второй видит реплику первого и может вцепиться.
5. Всё это стримится в UI через SSE (`/api/events`): статус Режиссёра, реплики по мере появления, обновления графа.

`POST /api/tick` — тот же раунд без реплики игрока (Режиссёру явно сказано сталкивать агентов между собой) плюс
энтропия: все живые теряют HP/дофамин. HP = 0 → **permadeath**, агент замолкает навсегда (до `POST /api/reset`).

`POST /api/schema` — Триада собирает структурированную Mermaid-схему текущей темы с аннотациями каждого.

## API

| Метод | Путь | Что делает |
|---|---|---|
| `GET` | `/api/state` | агенты (HP, дофамин, повестка), сообщения с `ego_thought`, граф + готовый Mermaid, последняя схема, последний план Режиссёра |
| `POST` | `/api/chat` `{message}` | полный раунд: Режиссёр → 1–2 агента → обновлённое состояние |
| `POST` | `/api/tick` | автономный тик (агенты говорят между собой) |
| `POST` | `/api/schema` (алиас `/api/synthesize-schema`) | сборка блок-схемы спора |
| `POST` | `/api/reset` | стереть комнату, заселить Триаду заново |
| `GET` | `/api/events` | SSE-поток живых событий раунда |
| `GET` | `/api/health` | режим (mock/live), модели |

## План «первого запуска» сегодня вечером

1. Вставь ключ, напиши: *«Короче, я хочу сделать стартап, но мне впадлу писать документацию, хочу сразу кодить»*.
2. Открой вкладку **Режиссёр** справа (или шторку на мобильном) — смотри `updated_agendas`: кого Режиссёр натравил и зачем.
3. Жми **🎲 Автономный тик** 2–3 раза: они забудут про тебя и начнут грызться между собой, а на графе полезут
   разноцветные узлы-конфликты.
4. **⚡ Собрать схему** — сухой остаток спора в виде блок-схемы с тремя язвительными аннотациями.

## Деплой под Telegram Mini Apps

```bash
cd frontend && VITE_API_URL= npm run build        # пусто = same-origin, бандл лежит рядом с бэкендом
cd ../backend && cargo build --release
scp target/release/triad-canvas-backend .env user@vps:/opt/triad/
scp -r ../frontend/dist user@vps:/opt/triad/frontend/dist
# на VPS: STATIC_DIR=./frontend/dist ./triad-canvas-backend  (за Caddy/nginx с TLS — Telegram требует https)
```

В BotFather: `/newapp` → URL `https://твой-домен`. `index.html` уже подключает `telegram-web-app.js`,
`src/telegram.ts` вызывает `ready()/expand()`, красит шапку в `#0D0E12` и даёт haptic-отклик на кнопках.
Если фронт хостится отдельно (Vercel/Pages) — просто задай `VITE_API_URL=https://api.твой-домен`; CORS на бэкенде permissive.

## Структура

```
backend/src/
  main.rs         старт, CORS, роутер, раздача dist
  config.rs       env → Config (mock-режим без ключа)
  models.rs       Agent, Message, GraphNode/Edge, DirectorOutput, RoomState, RoomEvent
  db.rs           пул, миграции, сид трёх персонажей, CRUD
  openrouter.rs   reqwest-клиент, prefill, extract_json
  mock_llm.rs     офлайн-заглушка LLM (для UI/пайплайна без ключа)
  prompts.rs      Режиссёр / анти-ассистентское ядро агента / синтез схемы
  director.rs     план хода, повестки, статы, мутации графа, fallback
  agent_runner.rs ход агента, парсинг <ego_thought>
  mermaid.rs      safe_id, sanitize_label, build_graph, classDef-раскраска
  handlers.rs     axum-роуты + SSE
  state.rs        AppState
frontend/src/
  App.tsx, hooks/useRoom.ts (SSE + REST), api.ts, telegram.ts, types.ts
  components/ TopBar · AgentCard · StatBar · ChatStream · MessageBubble · Composer
              GraphPanel · MermaidView · DirectorInspector · Drawer
```

## Тесты

```bash
cd backend && cargo test     # парсинг <ego_thought>, извлечение JSON, санитизация Mermaid, mock-режиссёр
```
