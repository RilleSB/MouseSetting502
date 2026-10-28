# ghub_rust 🦀

Легковесный, сверхбыстрый нативный аналог Logitech G HUB на **Rust** с графическим интерфейсом (Slint) и CLI.

Вместо сотен мегабайт Electron и фоновых служб (`lghub_agent`, `lghub_updater`, `lghub_system_tray`), этот проект напрямую обращается к протоколу **Logitech HID++ 2.0** через USB HID:
- **Размер бинарника**: компактный нативный исполняемый файл без внешних DLL-зависимостей
- **Потребление RAM**: ~15–25 МБ (включая аппаратный рендеринг GUI на FemtoVG/OpenGL)
- **Запуск и отклик**: доли миллисекунды, мгновенный отклик 60 FPS без фризов
- **UI & HID изоляция**: все аппаратные обращения вынесены в асинхронный воркер через каналы `mpsc`

---

## Поддерживаемые устройства

* **Logitech G502 HERO Gaming Mouse** (`VID: 0x046D`, `PID: 0xC08B`):
  * Чтение и изменение DPI на лету (100–25600 DPI, с шагом 50 и валидацией)
  * Управление подсветкой RGB LIGHTSYNC (логотип «G» и DPI-индикаторы):
    * Статический цвет (любой 24-bit HEX или быстрые пресеты)
    * Цветовой спектральный цикл (Spectrum Cycle)
    * Эффект дыхания (Breathing)
    * Полное отключение светодиодов
  * Инициализация Host Mode (`0x8100`) для программного управления без аппаратных переопределений
* **Logitech G435 Wireless Gaming Headset** (`VID: 0x046D`, `PID: 0x0ACB`):
  * Обнаружение Lightspeed-донгла и HID-интерфейсов гарнитуры

---

## Архитектура и надежность

1. **Строгий фильтр HID++ 2.0 пакетов**:
   - Фильтрация по `device_idx`, `feature_idx`, `function_id` и монотонному циклическому `software_id` (1..14).
   - Защита от коллизий с параллельно запущенным G HUB и асинхронными уведомлениями мыши.
   - Корректная обработка сообщений об ошибках (`feature_idx == 0xFF`).
2. **Безопасный движок макросов**:
   - Полная таблица сопоставления USB HID Usage Page 0x07 → Windows Virtual-Key (включая модификаторы Ctrl, Shift, Alt, Win, клавиши F1–F24, Numpad).
   - `TrackedState`: мгновенное отпускание всех зажатых кнопок и клавиш при остановке макроса или закрытии окна (гарантия отсутствия «залипания»).
   - Ограничение частоты циклов (минимум 10 мс) для защиты от 100% загрузки процессора.
3. **Прямое чтение базы G HUB**:
   - SQLite база `%LOCALAPPDATA%\LGHUB\settings.db` открывается с `busy_timeout` и очисткой NUL-байтов.
   - Кэширование профилей при старте приложения без постоянного перечитывания диска.

---

## Запуск GUI

Для запуска современного графического интерфейса достаточно запустить бинарник без аргументов:
```powershell
.\target\release\ghub_rust.exe
```

---

## Использование CLI

```powershell
# Запуск графического интерфейса:
.\target\release\ghub_rust.exe

# Просмотр информации об устройстве и активном DPI:
.\target\release\ghub_rust.exe --cli

# Изменение DPI (100–25600, шаг 50):
.\target\release\ghub_rust.exe --dpi 800
.\target\release\ghub_rust.exe --dpi 1200

# Управление подсветкой:
.\target\release\ghub_rust.exe --color 00FFCC
.\target\release\ghub_rust.exe --breathe 0088FF
.\target\release\ghub_rust.exe --cycle
.\target\release\ghub_rust.exe --off

# Сканирование подключенных устройств Logitech:
.\target\release\ghub_rust.exe --scan

# Список макросов и привязок кнопок из базы G HUB:
.\target\release\ghub_rust.exe --macros

# Тестовый запуск макроса:
.\target\release\ghub_rust.exe --run-macro lkm
```

---

## Сборка и запуск тестов

Сборка осуществляется с помощью Rust (`stable-x86_64-pc-windows-gnu` и MinGW `w64devkit`):

```powershell
# Проверка типов и синтаксиса:
cargo check

# Запуск встроенных unit-тестов:
cargo test

# Сборка оптимизированного релиза:
cargo build --release
```
