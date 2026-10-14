# TODO
- `direction_pin` in `IoPin{pin, direction_pin}` not needed for us, right? JTAG specific?
    let dir_swdio = p.PIN_12;   // SWDCLK__JCLK
    let dir_swclk = p.PIN_19;   // SWDIO_TMS
- integrate in `eurocard/pico/controller`

# GOALS
- use `rusty-probe` in `eurocard/pico/controller`
- optional: implement swd pio

# NOTES
- `flash.blocking_unique_id()` (get unique id from flash) not implemented for rp235xa