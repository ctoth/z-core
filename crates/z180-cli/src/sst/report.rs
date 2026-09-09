use super::*;

pub(super) fn print_text(report: &RunReport) {
    for excluded in &report.excluded {
        println!("EXCLUDED {}: {}", excluded.file, excluded.reason);
    }
    for file in &report.files {
        let status = if file.fail != 0 {
            "FAIL"
        } else if file.unimplemented != 0 {
            "UNIMPLEMENTED"
        } else {
            "PASS"
        };
        println!(
            "{status} {}: pass={} fail={} unimplemented={}",
            file.file, file.pass, file.fail, file.unimplemented
        );
        for failure in &file.failures {
            println!(
                "  {}: {} expected={} actual={}",
                failure.test, failure.field, failure.expected, failure.actual
            );
        }
    }
    if !report.census.is_empty() {
        for entry in &report.census {
            println!("CENSUS {}={}", entry.family, entry.cases);
        }
        println!(
            "CENSUS total={}",
            report.census.iter().map(|entry| entry.cases).sum::<usize>()
        );
    }
    println!(
        "SUMMARY pass={} fail={} unimplemented={} excluded={}",
        report.pass,
        report.fail,
        report.unimplemented,
        report.excluded.len()
    );
}
