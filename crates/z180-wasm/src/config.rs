use super::*;

pub(super) fn parse_config(value: Option<JsValue>) -> Result<MachineConfig, JsValue> {
    let mut config = MachineConfig::default();
    let Some(value) = value.filter(|value| !value.is_null() && !value.is_undefined()) else {
        return Ok(config);
    };
    let object = expect_object(value, "config")?;
    reject_unknown_keys(
        &object,
        &[
            "clockHz",
            "physAddrBits",
            "unmappedRead",
            "variant",
            "regions",
            "eventCapacity",
        ],
        "config",
    )?;

    if let Some(value) = optional_property(&object, "clockHz")? {
        config.clock_hz = expect_u32(value, "config.clockHz")?;
    }
    if let Some(value) = optional_property(&object, "physAddrBits")? {
        config.phys_addr_bits = expect_u8(value, "config.physAddrBits")?;
    }
    if let Some(value) = optional_property(&object, "unmappedRead")? {
        config.unmapped_read = expect_u8(value, "config.unmappedRead")?;
    }
    if let Some(value) = optional_property(&object, "variant")? {
        config.variant = match value.as_string().as_deref() {
            Some("Z80180") => Variant::Z80180,
            Some("Z8S180") => Variant::Z8S180,
            _ => return Err(js_error("config.variant must be 'Z80180' or 'Z8S180'")),
        };
    }
    if let Some(value) = optional_property(&object, "regions")? {
        config.regions = parse_regions(value)?;
    }
    if let Some(value) = optional_property(&object, "eventCapacity")? {
        config.event_capacity = expect_u32(value, "config.eventCapacity")? as usize;
    }
    Ok(config)
}

fn parse_regions(value: JsValue) -> Result<Vec<RegionDef>, JsValue> {
    if !Array::is_array(&value) {
        return Err(js_error("config.regions must be an array"));
    }
    Array::from(&value)
        .iter()
        .enumerate()
        .map(|(index, value)| parse_region(value, index))
        .collect()
}

fn parse_region(value: JsValue, index: usize) -> Result<RegionDef, JsValue> {
    let label = format!("config.regions[{index}]");
    let object = expect_object(value, &label)?;
    reject_unknown_keys(&object, &["base", "size", "kind", "data"], &label)?;
    let base = expect_u32(
        required_property(&object, "base", &label)?,
        &format!("{label}.base"),
    )?;
    let size = expect_u32(
        required_property(&object, "size", &label)?,
        &format!("{label}.size"),
    )?;
    let kind_value = required_property(&object, "kind", &label)?;
    let kind_name = kind_value
        .as_string()
        .ok_or_else(|| js_error(format!("{label}.kind must be a string")))?;
    let data = optional_property(&object, "data")?;
    let kind = match (kind_name.as_str(), data) {
        ("ram", None) => RegionKind::Ram,
        ("external", None) => RegionKind::External,
        ("rom", Some(data)) => RegionKind::Rom(expect_bytes(data, &format!("{label}.data"))?),
        ("rom", None) => return Err(js_error(format!("{label}.data is required for ROM"))),
        ("ram" | "external", Some(_)) => {
            return Err(js_error(format!("{label}.data is only valid for ROM")));
        }
        _ => {
            return Err(js_error(format!(
                "{label}.kind must be 'ram', 'rom', or 'external'"
            )));
        }
    };
    Ok(RegionDef { base, size, kind })
}

pub(super) fn parse_callbacks(value: Option<JsValue>, unmapped_read: u8) -> Result<JsBus, JsValue> {
    let mut bus = JsBus {
        unmapped_read,
        mem_read: None,
        mem_write: None,
        io_read: None,
        io_write: None,
    };
    let Some(value) = value.filter(|value| !value.is_null() && !value.is_undefined()) else {
        return Ok(bus);
    };
    let object = expect_object(value, "callbacks")?;
    reject_unknown_keys(
        &object,
        &["memRead", "memWrite", "ioRead", "ioWrite"],
        "callbacks",
    )?;
    bus.mem_read = optional_function(&object, "memRead")?;
    bus.mem_write = optional_function(&object, "memWrite")?;
    bus.io_read = optional_function(&object, "ioRead")?;
    bus.io_write = optional_function(&object, "ioWrite")?;
    Ok(bus)
}

fn expect_object(value: JsValue, label: &str) -> Result<Object, JsValue> {
    if !value.is_object() || Array::is_array(&value) {
        return Err(js_error(format!("{label} must be an object")));
    }
    value
        .dyn_into::<Object>()
        .map_err(|_| js_error(format!("{label} must be an object")))
}

fn reject_unknown_keys(object: &Object, allowed: &[&str], label: &str) -> Result<(), JsValue> {
    for key in Object::keys(object).iter() {
        let key = key
            .as_string()
            .ok_or_else(|| js_error(format!("{label} contains a non-string key")))?;
        if !allowed.contains(&key.as_str()) {
            return Err(js_error(format!("unknown {label} field: {key}")));
        }
    }
    Ok(())
}

fn optional_property(object: &Object, name: &str) -> Result<Option<JsValue>, JsValue> {
    let value = Reflect::get(object, &JsValue::from_str(name))?;
    Ok((!value.is_null() && !value.is_undefined()).then_some(value))
}

fn required_property(object: &Object, name: &str, label: &str) -> Result<JsValue, JsValue> {
    optional_property(object, name)?.ok_or_else(|| js_error(format!("{label}.{name} is required")))
}

fn optional_function(object: &Object, name: &str) -> Result<Option<Function>, JsValue> {
    optional_property(object, name)?
        .map(|value| {
            value
                .dyn_into::<Function>()
                .map_err(|_| js_error(format!("callbacks.{name} must be a function")))
        })
        .transpose()
}

fn expect_u8(value: JsValue, label: &str) -> Result<u8, JsValue> {
    let number = expect_integer(value, label)?;
    if number > f64::from(u8::MAX) {
        return Err(js_error(format!("{label} must be in 0..=255")));
    }
    Ok(number as u8)
}

pub(super) fn expect_u32(value: JsValue, label: &str) -> Result<u32, JsValue> {
    let number = expect_integer(value, label)?;
    if number > f64::from(u32::MAX) {
        return Err(js_error(format!("{label} must be in 0..=4294967295")));
    }
    Ok(number as u32)
}

fn expect_integer(value: JsValue, label: &str) -> Result<f64, JsValue> {
    let Some(number) = value.as_f64() else {
        return Err(js_error(format!("{label} must be an integer")));
    };
    if !number.is_finite() || number.fract() != 0.0 || number < 0.0 {
        return Err(js_error(format!("{label} must be a non-negative integer")));
    }
    Ok(number)
}

fn expect_bytes(value: JsValue, label: &str) -> Result<Vec<u8>, JsValue> {
    value
        .dyn_into::<Uint8Array>()
        .map(|array| array.to_vec())
        .map_err(|_| js_error(format!("{label} must be a Uint8Array")))
}

pub(super) fn js_error(message: impl AsRef<str>) -> JsValue {
    js_sys::Error::new(message.as_ref()).into()
}

pub(super) fn config_error(error: ConfigError) -> JsValue {
    js_error(error.to_string())
}
