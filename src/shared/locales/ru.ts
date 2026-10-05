// Russian UI strings (source of truth for keys).
export const ru = {
  // Palette
  paletteInput: "Что сделать с текстом?",
  paletteHint: "↑↓ выбор · Enter применить · Esc закрыть",
  paletteNoMatch: "Ничего не найдено. Измените запрос.",
  paletteEmptyTitle: "Сначала подключите нейросеть",
  paletteEmptyBody: "Укажите адрес, ключ и модель в настройках.",
  openSettings: "Открыть настройки",
  retry: "Повторить запрос",
  busy: "Обрабатываю…",
  paletteRecent: "Недавние",
  paletteAsk: "Напишите, что сделать с текстом",
  paletteOneoff: "Enter — выполнить как инструкцию: «{text}»",
  recentSave: "Сохранить как команду",
  recentSaved: "Сохранено",

  // Preview
  previewAccept: "Принять",
  previewCopy: "Копировать",
  previewCopied: "Скопировано",
  previewRetry: "Повторить",
  previewClose: "Закрыть",
  previewWaiting: "Думаю…",
  previewReady: "Результат готов",
  previewStreamError:
    "Ответ прервался. Показано то, что успело прийти. Текст в приложении не изменён.",

  // Errors
  errNetwork:
    "Нет соединения с сервером. Проверьте интернет и адрес провайдера.",
  errAuth: "Провайдер не принял ключ. Проверьте API-ключ в настройках.",
  errTimeout: "Нейросеть не ответила вовремя. Текст не изменён.",
  errRateLimit:
    "Провайдер ограничил число запросов. Подождите немного и повторите.",
  errNotFound: "Модель или адрес не найдены. Проверьте настройки провайдера.",

  // added in 01-05
  errOther: "Что-то пошло не так. Текст не изменён.",
  errFocusLost: "Не удалось вернуться в исходное окно. Текст не изменён.",
  errFocusLostCopied:
    "Не удалось вернуться в исходное окно. Результат в буфере — нажмите «Вставить туда» или Ctrl+V.",
  errSelectionChanged:
    "Выделение изменилось — результат скопирован, вставьте через Ctrl+V.",
  errReadOnlyCopied:
    "Сюда нельзя вставить текст. Результат скопирован — вставьте его через Ctrl+V туда, куда нужно.",
  pasteThere: "Вставить туда",
  errPasteBackFailed:
    "Не удалось вернуться в исходное окно. Результат в буфере — вставьте его через Ctrl+V.",
  errHotkeyBusy:
    "Главная клавиша занята другой программой. Выберите другую в настройках Чистовика.",

  // Capture
  captureEmpty: "В поле нет текста. Выделите текст и повторите.",
  capturePassword: "Это поле пароля — Чистовик его не трогает.",
  captureAdmin:
    "Окно запущено от администратора, Чистовик не может в него вставить текст. Скопируйте текст вручную или запустите Чистовик от администратора.",
  captureUnsupported:
    "Не удалось получить текст из этого приложения. Выделите текст и повторите.",

  // Settings validation
  validationUrl: "Введите адрес вида https://api.openai.com/v1",
  validationRequired: "Заполните это поле",
  saved: "Сохранено",
  showKey: "Показать ключ",
  hideKey: "Скрыть ключ",

  // Settings
  settingsTitle: "Чистовик",
  settingsSection: "Нейросеть",
  saveProvider: "Сохранить нейросеть",
  fieldName: "Название",
  fieldBaseUrl: "Адрес (Base URL)",
  fieldApiKey: "API-ключ",
  fieldModel: "Модель",
  keySaved: "Ключ сохранён. Оставьте поле пустым, чтобы не менять его.",
  keyPlaceholderSaved: "Ключ сохранён",
  keyInfo: "Ключ хранится в диспетчере учётных данных Windows",
  busyPosition: "Индикатор обработки",
  busyPosCursor: "Где был курсор",
  busyPosCenter: "По центру экрана",
  busyPosFollow: "За курсором",
  hotkeyInvalid: "Такое сочетание не подходит",
  hotkeyNoModifier: "Нужна клавиша Ctrl, Alt или Win",
  hotkeyUsedByMain: "Занято главным хоткеем палитры",
  hotkeyUsedByPrompt: "Уже у команды «{name}»",
  hotkeyUsedByOtherApp:
    "Не удалось занять: похоже, его использует другая программа",

  // Settings shell and tabs (phase 2)
  tabGeneral: "Общие",
  tabPrompts: "Промпты",
  tabProviders: "Провайдеры",
  tabHotkeys: "Хоткеи",
  tabHistory: "История",
  tabAbout: "О программе",
  resultMode: "Режим результата",
  modeImmediate: "Сразу",
  modePreview: "Превью",
  modeCombo: "Комбинация",
  modeImmediateHelp: "Сразу — текст заменяется без показа.",
  modePreviewHelp: "Превью — сначала показать результат, потом принять.",
  modeComboHelp:
    "Комбинация — хоткей команды заменяет сразу, выбор из палитры показывает превью.",
  palettePosition: "Позиция палитры",
  posCursor: "У курсора",
  posCaret: "У каретки",
  posCenter: "По центру",
  posCaretHelp:
    "Если в приложении не видно каретку, палитра откроется у курсора.",
  previewEditable: "Правка в превью",
  previewEditableLabel: "Можно править текст в превью",
  previewEditableHelp:
    "Перед вставкой текст в превью можно исправить вручную. Ctrl+Enter — принять.",
  focusLoss: "При потере фокуса во время превью",
  focusClose: "Закрывать",
  focusStay: "Оставаться поверх",

  // Prompts editor
  commands: "Команды",
  newCommand: "Новая",
  newCommandTitle: "Новая команда",
  hiddenGroup: "Скрытые ({n})",
  importBtn: "Импорт",
  exportBtn: "Экспорт",
  restoreBuiltins: "Вернуть встроенные",
  editorTitle: "Название",
  editorIcon: "Иконка",
  editorHotkey: "Клавиша",
  editorText: "Текст",
  hotkeyNone: "Не назначена",
  textHint:
    "Вставьте {{text}} там, где должен стоять выделенный текст. Если его нет, текст будет добавлен в конец.",
  reset: "Сбросить",
  hide: "Скрыть",
  show: "Показать",
  remove: "Удалить",
  cancel: "Отмена",
  confirmDelete: "Удалить команду «{name}» насовсем?",
  confirmReset:
    "Вернуть исходные название, иконку и текст? Ваши правки пропадут.",
  iconsNotFound: "Иконок не найдено",
  iconSearch: "Поиск иконки",
  dragHandle: "Перетащить",
  emptyTitle: "Команд пока нет",
  emptyBody: "Создайте свою команду кнопкой «+ Новая» или верните встроенные.",
  importResult: "Добавлено: {n}, пропущено повторов: {m}",
  importInvalid: "Файл не похож на экспорт Чистовика. Команды не изменены.",
  hotkeyDefaultFailed:
    "Не удалось занять клавишу {key}: она занята другой программой. Назначьте другую.",
  hotkeyNoModifierShort: "Добавьте Ctrl, Alt или Win",
  hotkeyUsedByOtherAppShort:
    "Занято другой программой. Выберите другое сочетание.",
  hotkeyUsedByMainShort: "Это главная клавиша палитры",
  askNoEditor: "Эта команда спрашивает инструкцию в палитре.",

  // Providers tab (phase 3)
  providers: "Провайдеры",
  addProvider: "Добавить",
  otherProvider: "Другой",
  defaultBadge: "По умолчанию",
  makeDefault: "Сделать основным",
  check: "Проверить",
  checking: "Проверяю…",
  refreshModels: "Обновить список",
  checkOk: "Подключение работает. Моделей в списке: {n}.",
  checkOkNoList:
    "Подключение работает. Список моделей сервер не отдаёт — впишите имя вручную.",
  keyOptionalLocal: "Для локального сервера ключ не нужен.",
  confirmDeleteProvider: "Удалить провайдера «{name}» и его ключ?",
  noProvidersTitle: "Провайдеров пока нет",
  noProvidersBody: "Нажмите «Добавить» и выберите сервис.",
  newProvider: "Новый провайдер",
  saveBtn: "Сохранить",

  // Model per command (phase 3)
  editorModel: "Модель",
  modelDefault: "Как по умолчанию",
  modelDefaultOf: "Как по умолчанию ({name} · {model})",
  modelPlaceholder: "Модель провайдера: {model}",
  providerRemoved: "Провайдер удалён — используется провайдер по умолчанию.",
  editorProvider: "Провайдер",

  // History tab (phase 3)
  historySearch: "Поиск по истории",
  historyEnabled: "Вести историю",
  historyLimit: "Хранить",
  historyLimitN: "{n} записей",
  historyLimitNone: "Без лимита",
  historyClear: "Очистить всё",
  historyClearConfirm: "Удалить всю историю? Это нельзя отменить.",
  historyEmpty: "История пуста",
  historyEmptyBody: "Здесь появятся успешные обработки: что было и что стало.",
  historyOff:
    "Запись истории выключена. Старые записи остаются, пока вы их не удалите.",
  historyNothingFound: "Ничего не найдено",
  historyBefore: "До",
  historyAfter: "После",
  historyCopyBefore: "Копировать до",
  historyCopyAfter: "Копировать после",
  historyDelete: "Удалить запись",
  historyMore: "Показать ещё",
  copied: "Скопировано",

  // Undo (phase 3)
  undoNothing:
    "Нечего отменять. Отменить можно только последнюю замену, один раз.",
  undoCopied:
    "Не удалось вернуть текст на место. Исходный текст скопирован в буфер — вставьте его через Ctrl+V.",
  hotkeyUsedByUndo: "Это клавиша отмены последней замены",
  hotkeysPalette: "Открыть палитру",
  hotkeysPaletteHelp:
    "Нажмите поле и нажмите новое сочетание с Ctrl, Alt или Win. Backspace возвращает Ctrl+Alt+Space.",
  hotkeySaveFailed: "Не удалось сохранить сочетание. Попробуйте ещё раз.",
  hotkeysUndo: "Отменить последнюю замену",
  hotkeysUndoHelp:
    "Возвращает исходный текст в то же окно. Работает один раз и даже при выключенной истории.",
  hotkeysCommands: "Клавиши команд назначаются на вкладке «Промпты».",

  // Tray / autostart (phase 4)
  autostartOffer:
    "Запускать Чистовик вместе с Windows? Он будет тихо ждать в трее, окна не откроются.",
  autostartEnable: "Включить автозапуск",
  autostart: "Запуск",
  autostartLabel: "Запускать вместе с Windows",
  autostartHelp: "При автозапуске окна не открываются — только иконка в трее.",

  // About / updates (phase 4)
  aboutTagline:
    "Выделил текст → хоткей → исправленный текст на месте. В любом приложении Windows.",
  aboutVersion: "Версия {v} · ранняя версия",
  aboutLicense: "Открытый код, лицензия MIT",
  aboutRepo: "Страница проекта на GitHub",
  updateAvailable: "Доступна версия {v}",
  updateInstall: "Обновить",
  updateInstalling: "Скачиваю и устанавливаю… Чистовик перезапустится сам.",
  updateCheck: "Проверить обновления",
  updateChecking: "Проверяю…",
  updateNone: "У вас последняя версия.",
  updateFailed:
    "Не удалось проверить обновления. Проверьте интернет и попробуйте позже.",
  updateInstallFailed:
    "Не удалось установить обновление. Попробуйте позже или скачайте его со страницы проекта.",

  // Language (phase 4)
  language: "Язык",
  langAuto: "Как в Windows",
  langRu: "Русский",
  langEn: "English",
};

export type StringKey = keyof typeof ru;
