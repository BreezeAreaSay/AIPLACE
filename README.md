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
* Бэкенд — один бинарник. На ноутбуке слушает `localhost:3000`; на VPS за $5 — тот же бинарник, тот же SQLite,
  плюс он сам отдаёт встроенный UI. Telegram Mini App просто указывает на его URL, а сервер по `initData`
  раскладывает пользователей по отдельным комнатам.

## Установка за минуту (готовый бинарник)

Один файл `triad` со встроенным UI, без Node/Rust на машине:

```bash
curl -fsSL https://raw.githubusercontent.com/BreezeAreaSay/AIPLACE/main/install.sh | sh
```

```powershell
irm https://raw.githubusercontent.com/BreezeAreaSay/AIPLACE/main/install.ps1 | iex
```

Установщик кладёт бинарник в `~/.triad/bin` (Windows: `%LOCALAPPDATA%\triad\bin`), добавляет его в PATH и сразу
запускает мастер `triad setup`, где ты вставляешь ключ OpenRouter. Дальше:

| Команда | Что делает |
|---|---|
| `triad` | запустить комнату → http://localhost:3000 (без ключа спросит его прямо в терминале) |
| `triad setup` | мастер: ключ, модель агентов, модель Режиссёра, порт → `~/.triad/config.env` |
| `triad key sk-or-v1-…` | записать ключ без мастера (`triad key` без аргумента — удалить) |
| `triad config` | где лежит конфиг и что в нём (ключ маскируется) |
| `triad update` | обновиться до последнего GitHub Release (`--check` — только проверить) |
| `triad --port 8080` | любой флаг/переменная из таблицы ниже |

Ключ можно вставить и **в UI** (⚙ в шапке или баннер «mock-режим»): он уходит в `~/.triad/config.env`, в браузер
никогда не возвращается, а менять его через UI разрешено только с той машины, где крутится сервер
(иначе на VPS любой открывший Mini App мог бы подменить ключ). Снять ограничение: `ALLOW_REMOTE_SETTINGS=true`.

При старте `triad` тихо проверяет GitHub Releases и показывает бейдж «доступна vX.Y.Z» в шапке UI
(отключить: `TRIAD_NO_UPDATE_CHECK=1`). База и конфиг живут в `~/.triad` (переопределить: `TRIAD_HOME`).

## Запуск из исходников (dev)

```bash
cd backend
cp .env.example .env          # ключ можно и не вставлять — будет MOCK-режим с шаблонными репликами
cargo run                     # бинарник собирается как `triad`; UI встроится, если есть frontend/dist
```

```bash
cd frontend
npm install
npm run dev                   # http://localhost:5173 — /api проксируется на :3000, hot reload
```

Собрать «как релиз» локально: `cd frontend && npm run build && cd ../backend && cargo build --release` →
`backend/target/release/triad` уже содержит UI.

### Переменные окружения

Приоритет: переменные процесса → `./.env` (dev) → `~/.triad/config.env` → умолчания.

| Переменная | По умолчанию | Зачем |
|---|---|---|
| `OPENROUTER_API_KEY` | — | Пусто → mock-режим (пайплайн работает, химии нет) |
| `OPENROUTER_MODEL` | `deepseek/deepseek-chat` | Модель агентов. Нужна поддержка assistant-prefill (deepseek / anthropic / gemini / llama) |
| `OPENROUTER_DIRECTOR_MODEL` | = `OPENROUTER_MODEL` | Быстрая JSON-модель для Режиссёра, напр. `google/gemini-2.5-flash` |
| `PORT` / `HOST` | `3000` / `0.0.0.0` | Куда слушать |
| `DATABASE_URL` | `sqlite://~/.triad/triad.db?mode=rwc` | Автомиграция и сид триады при первом старте |
| `STATIC_DIR` | — | Отдавать UI с диска вместо встроенного (dev без пересборки) |
| `ALLOW_REMOTE_SETTINGS` | `false` | Разрешить `POST /api/settings` не только с localhost |
| `TRIAD_NO_UPDATE_CHECK` | — | `1` — не ходить на GitHub за версией при старте |
| `TELEGRAM_BOT_TOKEN` | — | Включает публичный режим: проверка initData, комната на пользователя, бот `/start` |
| `WEBAPP_URL` | = `APP_URL` | Публичный https-адрес мини-аппа для кнопки в `/start` |
| `TELEGRAM_ADMIN_IDS` | — | Telegram user id, которым можно менять ключ из мини-аппа |
| `TELEGRAM_POLLING` | `true` | Отвечать на `/start` long-polling'ом |
| `ALLOW_ANON_ROOMS` | `false` | Пускать браузеры без Telegram в комнату `local` в публичном режиме |
| `CONTEXT_MESSAGES` | `14` | Сколько реплик видят Режиссёр и агенты |
| `MEMORY_EVERY` | `8` | Через сколько новых сообщений переписывать дневники агентов |
| `TICK_ENTROPY_HP` / `TICK_ENTROPY_DOPAMINE` | `2` / `3` | Сколько теряет каждый за автономный тик |

## Релизы (для мейнтейнера)

```bash
# 1. поднять version в backend/Cargo.toml (и Cargo.lock: cd backend && cargo update -p triad-canvas-backend)
# 2. закоммитить, затем:
git tag v0.2.0 && git push origin v0.2.0
```

`.github/workflows/release.yml` соберёт UI, встроит его в бинарники для Linux x64/arm64, macOS Intel/Apple Silicon
и Windows x64, и опубликует GitHub Release с `SHA256SUMS.txt`. Тег обязан совпадать с версией в `Cargo.toml`
(workflow падает иначе). Имена ассетов `triad-<ver>-<target>.tar.gz|zip` — их ждут `install.sh` и `triad update`.
CI (`ci.yml`) на каждый push гоняет `npm run build`, `cargo clippy -D warnings` и `cargo test`.

## Механики комнаты

| Механика | Как работает |
|---|---|
| **HP / дофамин / уважение** | Режиссёр оценивает, «покормил» ли ты профиль каждого агента; автономные тики жгут HP (энтропия). HP = 0 → permadeath: агент произносит последние слова и замолкает до `Новая комната`. |
| **Скрытые повестки** | Каждый ход LLM-Режиссёр выдумывает агентам новые психологические цели; видно в инспекторе и по кнопке «цели». |
| **Слово игрока (контракты)** | Пишешь «обещаю / сделаю до пятницы» — Режиссёр оформляет контракт с дедлайном, назначает **поручителя**, который ставит на тебя свои HP (10–30). Сдержал (отчитался в чат или ✓ во вкладке «Слово») — поручитель получает HP, все +уважение. Нарушил или вышел срок — поручитель теряет ставку, комната теряет к тебе уважение; поручитель может от этого умереть. Вкладка «Слово»: таймеры, ✓ / 💀 / ✖, форма «дать слово». |
| **Отношения агентов** | Матрица симпатий −100..100 по парам; Режиссёр двигает её каждый ход (поддержал → +, высмеял → −, сговор против игрока → + друг к другу). Агенты видят «ты в контрах с Софьей (−45)». Треугольник — в инспекторе Режиссёра. |
| **Долговременная память** | У каждого агента свой дневник от первого лица. После каждых `MEMORY_EVERY` (8) новых сообщений быстрая модель фоново переписывает его: мнение об игроке, его обещания, обиды на соседей, незакрытые споры. Дневник подмешивается в промпты агента и Режиссёра — ничего не забывается при прокрутке окна контекста. |
| **Лента событий** | Все механические факты (контракт создан/сдержан/нарушен, смерть) код пишет в `room_events`; Режиссёр обязан их отыграть на ближайшем ходу (поручитель реагирует первым). |
| **Живой граф и схема** | Узлы/рёбра из каждой реплики, раскраска по автору; «⚡ Собрать схему» — структурированная Mermaid-схема темы с аннотациями троих. |

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
| `GET` | `/api/state` | агенты, сообщения с `ego_thought`, граф + Mermaid, схема, план Режиссёра, контракты, отношения, дневники, события |
| `POST` | `/api/chat` `{message}` | полный раунд: Режиссёр → 1–2 агента → обновлённое состояние |
| `POST` | `/api/tick` | автономный тик (агенты говорят между собой) |
| `POST` | `/api/schema` (алиас `/api/synthesize-schema`) | сборка блок-схемы спора |
| `POST` | `/api/contracts/{id}/resolve` `{status: done\|failed\|cancelled}` | закрыть слово из UI; done/failed запускают раунд реакции (поручитель первым) |
| `POST` | `/api/reset` | стереть комнату, заселить Триаду заново |
| `GET` | `/api/events` | SSE-поток живых событий раунда |
| `GET` | `/api/health` | версия, режим (mock/live), модели |
| `GET`/`POST` | `/api/settings` | ключ (маскированный) и модели; `POST` — только с localhost или при `ALLOW_REMOTE_SETTINGS` |

## План «первого запуска» сегодня вечером

1. Вставь ключ, напиши: *«Короче, я хочу сделать стартап, но мне впадлу писать документацию, хочу сразу кодить»*.
2. Открой вкладку **Режиссёр** справа (или шторку на мобильном) — смотри `updated_agendas`: кого Режиссёр натравил и зачем.
3. Жми **🎲 Автономный тик** 2–3 раза: они забудут про тебя и начнут грызться между собой, а на графе полезут
   разноцветные узлы-конфликты.
4. **⚡ Собрать схему** — сухой остаток спора в виде блок-схемы с тремя язвительными аннотациями.

## Telegram Mini App

Когда задан `TELEGRAM_BOT_TOKEN`, сервер переходит в **публичный режим**:

- каждый запрос из мини-аппа несёт подписанный Telegram `initData` (`Authorization: tma …`); сервер проверяет HMAC
  по токену бота и свежесть (24 ч) — подделать пользователя нельзя;
- у каждого Telegram-пользователя **своя комната** — отдельный SQLite `~/.triad/rooms/tg_<id>.db` со своими агентами,
  контрактами, дневниками. Твоя старая база живёт в комнате `local` и открывается только с localhost;
- браузер без Telegram снаружи получает экран «Открыть в Telegram» (401 `telegram_required`); для отладки можно
  разрешить `ALLOW_ANON_ROOMS=true`;
- бот сам отвечает на `/start` кнопкой «⚡ Открыть комнату» (long polling, вебхуки и сертификаты не нужны);
- ключ и модели из мини-аппа может менять только `TELEGRAM_ADMIN_IDS` (твой user id).

### За 15 минут с ноутбука (без VPS)

1. **@BotFather** → `/newbot` → получи токен. Там же: `/mybots` → бот → *Bot Settings* → *Menu Button* — пока пропусти, URL появится на шаге 3.
2. **Туннель** до локального сервера (нужен https): [cloudflared](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/)
   ```bash
   cloudflared tunnel --url http://localhost:3000
   ```
   В выводе будет `https://<что-то>.trycloudflare.com` — это и есть адрес мини-аппа (меняется при каждом запуске туннеля).
3. **Настрой triad**:
   ```bash
   triad setup      # ключ OpenRouter → токен бота → WEBAPP_URL=https://<что-то>.trycloudflare.com → свой user id
   triad
   ```
   или вручную в `~/.triad/config.env`: `TELEGRAM_BOT_TOKEN=…`, `WEBAPP_URL=https://…`, `TELEGRAM_ADMIN_IDS=…`.
4. В **@BotFather**: *Menu Button* → вставь тот же https-URL (или `/newapp` — даст прямую ссылку `t.me/<bot>/<app>`).
5. Открой бота в Telegram → `/start` → кнопка. Готово: комната в телефоне, десктоп по-прежнему на http://localhost:3000.

### На VPS (постоянно)

```bash
ssh user@vps 'curl -fsSL https://raw.githubusercontent.com/BreezeAreaSay/AIPLACE/main/install.sh | TRIAD_NO_SETUP=1 sh'
ssh user@vps '~/.triad/bin/triad setup'     # ключ, токен бота, WEBAPP_URL=https://triad.example.com, admin id
```

`/etc/systemd/system/triad.service`:

```ini
[Service]
ExecStart=/home/user/.triad/bin/triad --no-prompt --host 127.0.0.1 --port 3000
Restart=always
User=user
[Install]
WantedBy=multi-user.target
```

Сверху Caddy: `triad.example.com { reverse_proxy 127.0.0.1:3000 }` — TLS он получит сам. Обновление:
`triad update && systemctl restart triad`. Если фронт хостится отдельно (Vercel/Pages) — задай `VITE_API_URL=https://api.…` при сборке; CORS permissive.

## Структура

```
backend/src/
  main.rs         CLI-диспетчер, старт сервера, CORS, роутер
  cli.rs          triad setup / key / config / update, подсказка ключа при первом запуске
  config.rs       ~/.triad/config.env, приоритеты env, горячие LlmSettings
  update.rs       самообновление с GitHub Releases + тихая проверка версии
  web.rs          встроенный UI (rust-embed, SPA-fallback); build.rs копирует frontend/dist
  telegram.rs     проверка initData (HMAC), бот /start (long polling)
  rooms.rs        комната на Telegram-пользователя: отдельный SQLite, ленивое открытие
  models.rs       Agent, Message, GraphNode/Edge, DirectorOutput, RoomState, RoomEvent
  db.rs           пул, миграции, сид трёх персонажей и их отношений, CRUD
  contracts.rs    слово игрока: дедлайны, поручители, последствия, события
  memory.rs       фоновое сворачивание дневников агентов
  openrouter.rs   reqwest-клиент, prefill, extract_json
  mock_llm.rs     офлайн-заглушка LLM (для UI/пайплайна без ключа)
  prompts.rs      Режиссёр / анти-ассистентское ядро агента / синтез схемы
  director.rs     план хода, повестки, статы, мутации графа, fallback
  agent_runner.rs ход агента, парсинг <ego_thought>
  mermaid.rs      safe_id, sanitize_label, build_graph, classDef-раскраска
  handlers.rs     axum-роуты + SSE + /api/settings (только localhost)
  state.rs        AppState
frontend/src/
  App.tsx, hooks/useRoom.ts (SSE + REST), api.ts, telegram.ts, types.ts
  components/ TopBar · AgentCard · StatBar · ChatStream · MessageBubble · Composer
              GraphPanel · MermaidView · DirectorInspector · Drawer · SettingsModal
              ContractsPanel · RelationsTriangle
.github/workflows/  ci.yml (build+clippy+test) · release.yml (теги v* → бинарники под 5 платформ)
install.sh / install.ps1   установка последнего релиза одной командой
```

## Тесты

```bash
cd backend && cargo test     # парсинг <ego_thought>, извлечение JSON, санитизация Mermaid, mock-режиссёр
```
