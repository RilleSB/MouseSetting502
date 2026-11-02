//! HID++ 0x8060: report intervals are in milliseconds, advertised as a bitmap.
use crate::hidpp::HidppDevice;

pub trait ReportRateTransport {
    fn report_rate_call(&self, function: u8, params: &[u8]) -> Result<Vec<u8>, String>;
}

impl ReportRateTransport for HidppDevice {
    fn report_rate_call(&self, function: u8, params: &[u8]) -> Result<Vec<u8>, String> {
        let index = self.get_feature_index(0x8060)?;
        Ok(self.call_function(index, function, params)?.params)
    }
}

pub fn read_report_rate(dev: &impl ReportRateTransport) -> Result<(Vec<u8>, u8), String> {
    let mask = *dev
        .report_rate_call(0, &[])?
        .first()
        .ok_or("Нет списка частот опроса")?;
    let supported: Vec<u8> = (1..=8).filter(|ms| mask & (1 << (ms - 1)) != 0).collect();
    let current = *dev
        .report_rate_call(1, &[])?
        .first()
        .ok_or("Нет текущей частоты опроса")?;
    if !supported.contains(&current) {
        return Err(format!(
            "Мышь сообщила неизвестный интервал опроса: {current} мс"
        ));
    }
    Ok((supported, current))
}

pub fn set_report_rate(
    dev: &impl ReportRateTransport,
    interval: u8,
) -> Result<(Vec<u8>, u8), String> {
    let (supported, current) = read_report_rate(dev)?;
    if !supported.contains(&interval) {
        return Err("Эта частота не поддерживается мышью".into());
    }
    if current != interval {
        dev.report_rate_call(2, &[interval])?;
    }
    let (_, confirmed) = read_report_rate(dev)?;
    if confirmed != interval {
        return Err(format!(
            "Мышь подтвердила интервал {confirmed} мс вместо {interval} мс"
        ));
    }
    Ok((supported, confirmed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    type ExpectedCall = (u8, Vec<u8>, Vec<u8>);
    struct Script(Mutex<VecDeque<ExpectedCall>>);
    impl ReportRateTransport for Script {
        fn report_rate_call(&self, function: u8, params: &[u8]) -> Result<Vec<u8>, String> {
            let (expected, payload, result) = self.0.lock().unwrap().pop_front().unwrap();
            assert_eq!((function, params), (expected, payload.as_slice()));
            Ok(result)
        }
    }
    fn script(calls: Vec<ExpectedCall>) -> Script {
        Script(Mutex::new(calls.into()))
    }
    #[test]
    fn reads_report_intervals_from_bitmap() {
        let mouse = script(vec![(0, vec![], vec![0x8b]), (1, vec![], vec![2])]);
        assert_eq!(read_report_rate(&mouse).unwrap(), (vec![1, 2, 4, 8], 2));
    }
    #[test]
    fn writes_only_supported_interval_and_confirms() {
        let mouse = script(vec![
            (0, vec![], vec![0x8b]),
            (1, vec![], vec![1]),
            (2, vec![4], vec![]),
            (0, vec![], vec![0x8b]),
            (1, vec![], vec![4]),
        ]);
        assert_eq!(set_report_rate(&mouse, 4).unwrap().1, 4);
        let invalid = script(vec![(0, vec![], vec![0x8b]), (1, vec![], vec![1])]);
        assert!(set_report_rate(&invalid, 3).is_err());
    }
    #[test]
    fn rejects_unconfirmed_change() {
        let mouse = script(vec![
            (0, vec![], vec![0x8b]),
            (1, vec![], vec![1]),
            (2, vec![4], vec![]),
            (0, vec![], vec![0x8b]),
            (1, vec![], vec![1]),
        ]);
        assert!(set_report_rate(&mouse, 4).is_err());
    }
}
