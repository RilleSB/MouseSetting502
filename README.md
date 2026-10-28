# ghub_rust 🦀

Легковесный, сверхбыстрый нативный аналог Logitech G HUB на **Rust**.

Вместо сотен мегабайт Electron и фоновых служб (`lghub_agent`, `lghub_updater`, `lghub_system_tray`), этот проект напрямую обращается к протоколу **Logitech HID++ 2.0** через USB HID:
- **Размер бинарника**: всего ~1.3 МБ
- **Потребление RAM**: < 5 МБ в активном режиме
- **Запуск и отклик**: доли миллисекунды (без задержек и подвисаний)

---

## Поддерживаемые устройства

* **Logitech G502 HERO Gaming Mouse** (`VID: 0x046D`, `PID: 0xC08B`)
  * Чтение и изменение DPI на лету (100–25600 DPI, шаг 50)
  * Управление подсветкой RGB LIGHTSYNC (логотип и DPI-индикаторы):
    * Статический цвет (любой 24-bit HEX)
    * Цветовой спектральный цикл (Spectrum Cycle)
    * Эффект дыхания (Breathing)
    * Полное отключение светодиодов
  * Обнаружение всех 18 аппаратных функций HID++ (Onboard Memory, Profiles, Button Events)
* **Logitech G435 Wireless Gaming Headset** (`VID: 0x046D`, `PID: 0x0ACB`)
  * Обнаружение Lightspeed-донгла и HID-интерфейсов гарнитуры

---

## Использование (CLI)

```powershell
# Показать текущую модель мыши и активный DPI:
.\target\release\ghub_rust.exe

# Сканировать подключенную периферию Logitech:
.\target\release\ghub_rust.exe --scan

# Установить DPI:
.\target\release\ghub_rust.exe --dpi 800
.\target\release\ghub_rust.exe --dpi 1600

# Установить статический цвет логотипа:
.\target\release\ghub_rust.exe --color 00FFCC   # Бирюзовый
.\target\release\ghub_rust.exe --color FF0055   # Неоновый розовый
.\target\release\ghub_rust.exe --color 7A00FF   # Фиолетовый

# Включить плавный радужный цикл (Spectrum Cycle):
.\target\release\ghub_rust.exe --cycle

# Включить пульсацию (Breathing):
.\target\release\ghub_rust.exe --breathe 0088FF

# Полностью выключить подсветку:
.\target\release\ghub_rust.exe --off
```

---

## Структура кода

```
src/
├── main.rs                 # CLI интерфейс и диспетчер команд
└── hidpp/
    ├── mod.rs
    ├── packet.rs           # Парсер и сериализатор пакетов HID++ 2.0
    ├── device.rs           # Потокобезопасная обертка над HidDevice и кэш фич
    └── features/           # Реализации конкретных фич Logitech:
        ├── name.rs         # 0x0005: Чтение названия модели
        ├── dpi.rs          # 0x2201: Чтение и переключение DPI сенсора
        └── rgb.rs          # 0x8070: Управление RGB подсветкой зон
```
