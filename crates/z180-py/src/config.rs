use super::*;

pub(super) fn config_error(error: ConfigError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

pub(super) fn parse_config(config: Option<&Bound<'_, PyDict>>) -> PyResult<MachineConfig> {
    let mut parsed = MachineConfig::default();
    let Some(config) = config else {
        return Ok(parsed);
    };

    for (key, value) in config.iter() {
        let key: String = key.extract()?;
        match key.as_str() {
            "clock_hz" => parsed.clock_hz = value.extract()?,
            "phys_addr_bits" => parsed.phys_addr_bits = value.extract()?,
            "unmapped_read" => parsed.unmapped_read = value.extract()?,
            "variant" => {
                let variant: String = value.extract()?;
                parsed.variant = match variant.as_str() {
                    "Z80180" => Variant::Z80180,
                    "Z8S180" => Variant::Z8S180,
                    _ => {
                        return Err(PyValueError::new_err(
                            "variant must be 'Z80180' or 'Z8S180'",
                        ));
                    }
                };
            }
            "regions" => parsed.regions = parse_regions(value.cast::<PyList>()?)?,
            "event_capacity" => parsed.event_capacity = value.extract()?,
            _ => {
                return Err(PyValueError::new_err(format!(
                    "unknown config field: {key}"
                )));
            }
        }
    }
    Ok(parsed)
}

fn parse_regions(regions: &Bound<'_, PyList>) -> PyResult<Vec<RegionDef>> {
    regions
        .iter()
        .map(|item| parse_region(item.cast::<PyDict>()?))
        .collect()
}

fn parse_region(region: &Bound<'_, PyDict>) -> PyResult<RegionDef> {
    for (key, _) in region.iter() {
        let key: String = key.extract()?;
        if !matches!(key.as_str(), "base" | "size" | "kind" | "data") {
            return Err(PyValueError::new_err(format!(
                "unknown region field: {key}"
            )));
        }
    }

    let base: u32 = region
        .get_item("base")?
        .ok_or_else(|| PyKeyError::new_err("base"))?
        .extract()?;
    let size: u32 = region
        .get_item("size")?
        .ok_or_else(|| PyKeyError::new_err("size"))?
        .extract()?;
    let kind_name: String = region
        .get_item("kind")?
        .ok_or_else(|| PyKeyError::new_err("kind"))?
        .extract()?;
    let data = region.get_item("data")?;
    let kind = match kind_name.as_str() {
        "ram" => {
            reject_data(&data, "ram")?;
            RegionKind::Ram
        }
        "rom" => {
            let data = data.ok_or_else(|| PyValueError::new_err("ROM region requires data"))?;
            RegionKind::Rom(data.extract()?)
        }
        "external" => {
            reject_data(&data, "external")?;
            RegionKind::External
        }
        _ => {
            return Err(PyValueError::new_err(
                "region kind must be 'ram', 'rom', or 'external'",
            ));
        }
    };
    Ok(RegionDef { base, size, kind })
}

fn reject_data(data: &Option<Bound<'_, PyAny>>, kind: &str) -> PyResult<()> {
    if data.is_some() {
        return Err(PyValueError::new_err(format!(
            "{kind} region does not accept data"
        )));
    }
    Ok(())
}

pub(super) fn remap_kind(kind: &str, data: Option<Vec<u8>>) -> PyResult<RegionKind> {
    match (kind, data) {
        ("ram", None) => Ok(RegionKind::Ram),
        ("rom", Some(data)) => Ok(RegionKind::Rom(data)),
        ("external", None) => Ok(RegionKind::External),
        ("rom", None) => Err(PyValueError::new_err("ROM remap requires data")),
        ("ram" | "external", Some(_)) => Err(PyValueError::new_err(format!(
            "{kind} remap does not accept data"
        ))),
        _ => Err(PyValueError::new_err(
            "kind must be 'ram', 'rom', or 'external'",
        )),
    }
}
