use std::time::Duration;

use crate::circular_buffer::CircularBuffer;

const CIRCULAR_BUFFER_SIZE: usize = 60;

/// Network informations about a connection.
#[derive(Debug, Default, Clone, Copy)]
pub struct NetworkInfo {
    /// Round-trip Time
    pub rtt: f32,
    /// Sent kilobits per second.
    pub sent_kbps: f32,
    /// Received kilobits per second.
    pub received_kbps: f32,
    pub packet_loss: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct PacketInfo {
    time: Duration,
    size: usize,
}

impl PacketInfo {
    fn new(time: Duration, size: usize) -> Self {
        Self { time, size }
    }
}

#[derive(Debug)]
pub struct BandwidthInfo {
    packets_sent: CircularBuffer<CIRCULAR_BUFFER_SIZE, PacketInfo>,
    packets_received: CircularBuffer<CIRCULAR_BUFFER_SIZE, PacketInfo>,
    bandwidth_smoothing_factor: f32,
    /// Sent kilobits per second.
    pub sent_kbps: f32,
    /// Received kilobits per second.
    pub received_kbps: f32,
}

impl<const N: usize> CircularBuffer<N, PacketInfo> {
    pub fn kilobits_per_second(&self) -> f32 {
        let mut start = Duration::MAX;
        let mut end = Duration::ZERO;
        let mut bytes_sent = 0;
        for packet_info in self.queue.iter() {
            if packet_info.size == 0 {
                continue;
            }
            if packet_info.time < start {
                start = packet_info.time;
            }
            if packet_info.time > end {
                end = packet_info.time;
            }

            bytes_sent += packet_info.size;
        }

        if start >= end {
            return 0.0;
        }

        let milli_seconds = (end - start).as_secs_f32() * 1000.0;
        (bytes_sent * 8) as f32 / milli_seconds
    }
}

impl BandwidthInfo {
    pub fn new(bandwidth_smoothing_factor: f32) -> Self {
        Self {
            packets_sent: Default::default(),
            packets_received: Default::default(),
            sent_kbps: 0.0,
            received_kbps: 0.0,
            bandwidth_smoothing_factor,
        }
    }

    pub fn sent_packet(&mut self, time: Duration, size: usize) {
        self.packets_sent.push(PacketInfo::new(time, size));
    }

    pub fn received_packet(&mut self, time: Duration, size: usize) {
        self.packets_received.push(PacketInfo::new(time, size));
    }

    pub fn update_metrics(&mut self) {
        let sent_kbps = self.packets_sent.kilobits_per_second();
        if self.sent_kbps == 0.0 || self.sent_kbps < f32::EPSILON {
            self.sent_kbps = sent_kbps;
        } else {
            self.sent_kbps += (sent_kbps - self.sent_kbps) * self.bandwidth_smoothing_factor;
        }

        let received_kbps = self.packets_received.kilobits_per_second();
        if self.received_kbps == 0.0 || self.received_kbps < f32::EPSILON {
            self.received_kbps = received_kbps;
        } else {
            self.received_kbps += (received_kbps - self.received_kbps) * self.bandwidth_smoothing_factor;
        };
    }
}
