# ghub_rust 🦀

Легковесный, сверхбыстрый нативный аналог Logitech G HUB на **Rust**.

Вместо сотен мегабайт Electron и фоновых служб (`lghub_agent`, `lghub_updater`, `lghub_system_tray`), этот проект напрямую обращается к протоколу **Logitech HID++ 2.0** через USB HID:
- **Размер бинарника**: всего ~1.8 МБ
- **Потребление RAM**: < 8 МБ в активном режиме
- **Запуск и отклик**: доли миллисекунды (без задержек и подвисаний)

---

## Возможности

* **Импорт настроек и макросов из оригинального G HUB**:
  * Прямое чтение SQLite базы данных `%LOCALAPPDATA%\LGHUB\settings.db`
  * Автоматический парсинг профилей, назначений кнопок мыши и макросов (одиночные нажатия, цепочки, автокликеры / Toggle)
  * Встроенный движок выполнения макросов на базе Win32 `SendInput`
* **Logitech G502 HERO Gaming Mouse** (`VID: 0x046D`, `PID: 0xC08B`):
  * Чтение и изменение DPI на лету (100–25600 DPI, шаг 50)
  * Управление подсветкой RGB LIGHTSYNC (логотип «G» и DPI-индикаторы):
    * Статический цвет (любой 24-bit HEX)
    * Цветовой спектральный цикл (Spectrum Cycle)
    * Эффект дыхания (Breathing)
    * Полное отключение светодиодов
  * Обнаружение всех 18 аппаратных функций HID++ (Onboard Memory, Profiles, Button Events)
* **Logitech G435 Wireless Gaming Headset** (`VID: 0x046D`, `PID: 0x0ACB`):
  * Обнаружение Lightspeed-донгла и HID-интерфейсов гарнитуры

---

## Использование (CLI)

```powershell
# Показать текущую модель мыши и активный DPI:
.\target\release\ghub_rust.exe

# Прочитать все макросы и привязки кнопок из базы G HUB:
.\target\release\ghub_rust.exe --macros

# Протестировать выполнение макроса (например, автокликер lkm):
.\target\release\ghub_rust.exe --run-macro lkm
.\target\release\ghub_rust.exe --run-macro pkm

# Установить DPI:
.\target\release\ghub_rust.exe --dpi 800

# Установить цвет логотипа мыши:
.\target\release\ghub_rust.exe --color 00FFCC
.\target\release\ghub_rust.exe --color FF0000

# Включить радужный спектральный цикл:
.\target\release\ghub_rust.exe --cycle

# Сканировать устройства Logitech:
.\target\release\ghub_rust.exe --scan
```
