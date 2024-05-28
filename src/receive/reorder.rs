//! Worker for grouping packets according to their block numbers to handle potential UDP packets
//! reordering

use std::time::Instant;

use log::{trace, warn};
use metrics::counter;
use raptorq::EncodingPacket;

use crate::protocol::{Header, MessageType, FIRST_BLOCK_ID, FIRST_SESSION_ID};

// if MAX is reached (diff en first block and last block) => force flush
const MAX_ACTIVE_QUEUES: usize = 20;

// how much time should we wait before allowing force decoding (in milliseconds)
const MAX_DELAY_MS: u128 = 500;

// how much time should we wait with no new block before changing session (in milliseconds)
//const MAX_SESSION_DELAY_MS: u128 = 10000;

#[derive(Eq, PartialEq, Copy, Clone)]
enum FlushCondition {
    Nothing,
    SessionExpired,
    BlockExpired,
    BlockOverflow,
    BlockComplete,
}

#[derive(Clone)]
struct Block {
    /// list of packets for this block
    packets: Vec<EncodingPacket>,
    /// last update for current block id // TODO il faut un last timestamp par queue
    last_timestamp: Instant,
    /// capacity: maximum number of packets possible : nb normal + nb repair packets
    /// could be factorized to gain a bit of memory but code will be more complex
    capacity: usize,
    /// loop count : number of times we reached this block
    loop_count: usize,
    /// used
    used: bool,
    /// slot / block id
    id: u8,
}

impl Block {
    fn new(id: u8, capacity: usize) -> Self {
        Self {
            // build a packet queue with right capacity.
            // This capacity is imported because it is tested after (see swap or full methods)
            packets: Vec::with_capacity(capacity),
            last_timestamp: Instant::now(),
            capacity,
            loop_count: 0,
            used: false,
            id,
        }
    }

    fn push(&mut self, packet: EncodingPacket, last_timestamp: Instant) {
        self.packets.push(packet);
        self.used = true;
        self.last_timestamp = last_timestamp;
    }

    fn clear(&mut self) {
        self.packets.clear();
        self.loop_count = 0;
        self.used = false;
    }

    fn pop(&mut self) -> Option<Vec<EncodingPacket>> {
        trace!("reorder: block.pop: len: {}", self.packets.len(),);

        self.loop_count += 1;
        self.used = false;
        Some(self.swap())
    }

    /// return true if all packets are stored for this block
    fn full(&self) -> bool {
        trace!(
            "reorder: block.full: len {} capacity {}",
            self.packets.len(),
            self.packets.capacity()
        );
        self.packets.len() == self.capacity
    }

    /// swap current queue with a new one and return current queue containing packets
    fn swap(&mut self) -> Vec<EncodingPacket> {
        // swap queues to return self.queue
        // allocate a new queue
        let mut queue = Vec::with_capacity(self.packets.capacity());
        std::mem::swap(&mut self.packets, &mut queue);

        queue
    }

    fn loop_count(&self) -> usize {
        self.loop_count
    }

    fn elapsed(&self) -> u128 {
        self.last_timestamp.elapsed().as_millis()
    }

    fn used(&self) -> bool {
        self.used
    }

    fn len(&self) -> usize {
        self.packets.len()
    }

    pub fn id(&self) -> usize {
        self.id as usize + self.loop_count() * 256
    }
}

struct Session {
    /// current block to decode
    current_block: u8,
    /// latest block received
    latest_block: usize,
    /// list of blocks, containing packets to reorder
    queues: Vec<Block>,
    /// if we know what is the last block id for this session (set when we receive 'end' flag)
    end_block: Option<u8>,
    /// this session id (const) : mainly used to recreate header
    session: u8,
    /// last timestamp
    last_timestamp: Instant,
    /// active = packets received for this session
    active: bool,
    /// session_expiration_delay from config
    session_expiration_delay: usize,
}

impl Session {
    /// capacity: maximum number of packets possible : nb normal + nb repair packets
    pub fn new(session: u8, capacity: usize, session_expiration_delay: usize) -> Self {
        let queues = (0..=u8::MAX as usize)
            .map(|i| Block::new(i as u8, capacity))
            .collect();
        Self {
            current_block: FIRST_BLOCK_ID,
            queues,
            end_block: None,
            latest_block: FIRST_BLOCK_ID as usize,
            session,
            last_timestamp: Instant::now(),
            active: false,
            session_expiration_delay,
        }
    }

    fn elapsed(&self) -> u128 {
        self.last_timestamp.elapsed().as_millis()
    }

    /// Add a received packet to this session
    ///
    /// Return true if we start filling a new queue
    pub fn push(&mut self, block_id: u8, packet: EncodingPacket) {
        // update last timestamp for every inserted packet
        self.last_timestamp = Instant::now();
        self.active = true;

        let block = &mut self.queues[block_id as usize];

        block.push(packet, self.last_timestamp);

        let real_block_id = block.id();

        // update latest block
        if self.latest_block < real_block_id {
            self.latest_block = real_block_id;
        }
    }

    /// Clears the session, removing/reset all values.
    ///
    /// Note that this method has no effect on the allocated capacity
    ///
    pub fn clear(&mut self) {
        self.current_block = FIRST_BLOCK_ID;
        self.latest_block = FIRST_BLOCK_ID as usize;
        self.queues.iter_mut().for_each(|q| q.clear());
        self.end_block = None;
        self.active = false;
    }

    pub fn check_flush_conditions(&self) -> FlushCondition {
        trace!(
            "reorder: check session {} block {} active {}",
            self.session,
            self.current_block,
            self.active
        );

        if !self.active {
            trace!("condition : inactive");
            counter!("reorder_flush_nothing_inactive").increment(1);
            return FlushCondition::Nothing;
        }

        // queue full <= this first (nominal)
        let current_block = &self.queues[self.current_block as usize];

        if !current_block.used() {
            // we don't have next block (maybe we lost it completly. check is last timestamp is
            // very old
            if self.elapsed() as usize > self.session_expiration_delay {
                trace!("condition : session expired");
                counter!("reorder_flush_session_expired").increment(1);
                return FlushCondition::SessionExpired;
            }
            return FlushCondition::Nothing;
        }

        if current_block.full() {
            trace!("condition : block complete");
            counter!("reorder_flush_block_complete").increment(1);
            return FlushCondition::BlockComplete;
        }

        // check we don't have too many blocks inflight
        let real_current_id = current_block.id();
        if self.latest_block - real_current_id >= MAX_ACTIVE_QUEUES {
            trace!(
                "condition : block {} len {} returned: max active queues",
                self.current_block,
                current_block.len()
            );
            counter!("reorder_flush_block_overflow").increment(1);
            return FlushCondition::BlockOverflow;
        }

        // do not return current block id if last insertion time is too close
        if current_block.elapsed() > MAX_DELAY_MS {
            trace!(
                "condition : block {} len {} returned: max delay",
                self.current_block,
                current_block.len()
            );
            counter!("reorder_flush_block_expired").increment(1);
            return FlushCondition::BlockExpired;
        }

        trace!("condition : nothing");
        counter!("reorder_flush_nothing").increment(1);
        FlushCondition::Nothing
    }

    pub fn pop_first(&mut self) -> Option<(MessageType, u8, u8, Vec<EncodingPacket>)> {
        let mut flags = MessageType::Data;
        let current_block_id = self.current_block;

        let queue = &mut self.queues[current_block_id as usize];

        if let Some(packets) = queue.pop() {
            if let Some(end_block) = self.end_block {
                if current_block_id == end_block {
                    flags |= MessageType::End;
                }
            }

            self.incr_block();

            Some((flags, self.session, current_block_id, packets))
        } else {
            None
        }
    }

    pub fn end_block(&mut self, block_id: u8) {
        if self.end_block.is_none() {
            self.end_block = Some(block_id);
        }
    }

    fn incr_block(&mut self) {
        trace!("increase block count: {} + 1", self.current_block);
        if self.current_block == u8::MAX {
            self.current_block = FIRST_BLOCK_ID;
        } else {
            self.current_block += 1;
        }
    }
}

pub struct Reorder {
    sessions: Vec<Session>,
    current_session: u8,
}

impl Reorder {
    pub fn new(
        nb_normal_packets: usize,
        nb_repair_packets: usize,
        session_expiration_delay: usize,
    ) -> Self {
        let capacity = nb_normal_packets + nb_repair_packets;
        let sessions: Vec<Session> = (0..=u8::MAX as usize)
            .map(|session_id| Session::new(session_id as u8, capacity, session_expiration_delay))
            .collect();

        Self {
            current_session: FIRST_SESSION_ID,
            sessions,
        }
    }

    fn session_mut(&mut self, session_id: u8) -> &mut Session {
        &mut self.sessions[session_id as usize]
    }

    fn session(&self, session_id: u8) -> &Session {
        &self.sessions[session_id as usize]
    }

    pub fn push(
        &mut self,
        header: Header,
        packet: EncodingPacket,
    ) -> Option<(MessageType, u8, u8, Vec<EncodingPacket>)> {
        // first process info from header
        if header.message_type().contains(MessageType::End) {
            self.session_mut(header.session()).end_block(header.block());
        }

        // then store received packet
        self.store_packet(header, packet);

        // check if we finished what we were waiting for
        self.reorder_finish()
    }

    fn store_packet(&mut self, header: Header, packet: EncodingPacket) {
        let session_id = header.session();
        let payload_id = packet.payload_id();
        let message_block_id = payload_id.source_block_number();

        // TODO : keep only one
        assert_eq!(message_block_id, header.block());

        trace!(
            "reorder: store packet session {session_id} block {message_block_id} part {}",
            header.seq()
        );

        let session = self.session_mut(session_id);

        session.push(message_block_id, packet);
    }

    fn process_flush(
        &mut self,
        reason: FlushCondition,
    ) -> Option<(MessageType, u8, u8, Vec<EncodingPacket>)> {
        match reason {
            FlushCondition::Nothing => None,
            FlushCondition::SessionExpired => {
                let session = self.session_mut(self.current_session);
                session.clear();
                self.incr_session();
                None
            }
            FlushCondition::BlockExpired
            | FlushCondition::BlockOverflow
            | FlushCondition::BlockComplete => {
                let session = self.session_mut(self.current_session);
                match session.pop_first() {
                    None => {
                        if reason == FlushCondition::BlockComplete {
                            warn!("Impossible case : block complete without packets");
                        }
                        None
                    }
                    Some(ret) => {
                        if ret.0.contains(MessageType::End) {
                            trace!("reorder: pop last block of a session, going to next session");
                            session.clear();
                            self.incr_session();
                        }

                        Some(ret)
                    }
                }
            }
        }
    }

    fn reorder_finish(&mut self) -> Option<(MessageType, u8, u8, Vec<EncodingPacket>)> {
        loop {
            let session = self.session(self.current_session);

            let reason = session.check_flush_conditions();
            let ret = self.process_flush(reason);
            if ret.is_some() {
                return ret;
            }

            if reason == FlushCondition::SessionExpired {
                assert!(ret.is_none());

                continue;
            }

            return None;
        }
    }

    /// return the oldest stored block queue
    /// étrange cette api avec force : faudrait appeler le check flush et seulement le pop ensuite
    /// ?
    pub fn pop_first(&mut self) -> Option<(MessageType, u8, u8, Vec<EncodingPacket>)> {
        self.reorder_finish()
    }

    fn incr_session(&mut self) {
        trace!("increase session count: {} + 1", self.current_session);
        if self.current_session == u8::MAX {
            self.current_session = FIRST_SESSION_ID;
        } else {
            self.current_session += 1;
        }
    }

    /// diode-send is restarted, so we have to flush/reset all queues
    pub fn clear(&mut self) {
        self.sessions.iter_mut().for_each(|s| s.clear());
        self.current_session = FIRST_SESSION_ID;
    }

    /// we miss diode-send init packet, so initialize reorder on the current session
    pub fn init(&mut self, header: Header) {
        self.current_session = header.session();
    }
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    //use super::*;

    use std::time::Duration;

    use raptorq::{EncodingPacket, PayloadId};

    use crate::{
        protocol::{Header, MessageType},
        receive::reorder::{MAX_ACTIVE_QUEUES, MAX_DELAY_MS},
    };

    use super::Reorder;

    fn build_packet(flags: MessageType, session: u8, block: u8) -> (Header, EncodingPacket) {
        let header = Header::new(flags, session, block);
        let packet = EncodingPacket::new(PayloadId::new(block, 0), vec![]);
        (header, packet)
    }

    #[test]
    fn test_single_succeed() {
        // prepare data
        let (header, packet) = build_packet(MessageType::End, 0, 0);
        let mut reorder = Reorder::new(1, 0, 1);

        // must succeed
        let (flags, session, block, packet) = reorder.push(header, packet).unwrap();
        assert!(flags.contains(MessageType::End));
        assert_eq!(session, 0);
        assert_eq!(block, 0);
        assert_eq!(packet.len(), 1);
    }

    #[test]
    fn test_single_block_fail() {
        // prepare data, starting with a block id != 0 => we wait for 0
        let (header, packet) = build_packet(MessageType::End, 0, 1);
        let mut reorder = Reorder::new(1, 0, 1);

        // must succeed
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());
    }

    #[test]
    fn test_single_session_fail() {
        // prepare data
        let header = Header::new(MessageType::End, 1, 0);
        let packet = EncodingPacket::new(PayloadId::new(0, 0), vec![]);
        let mut reorder = Reorder::new(1, 0, 1);

        // must fail
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());
    }

    #[test]
    fn test_single_block_and_session_fail() {
        // prepare data
        let header = Header::new(MessageType::End, 1, 1);
        let packet = EncodingPacket::new(PayloadId::new(1, 0), vec![]);
        let mut reorder = Reorder::new(1, 0, 1);

        // must fail
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());
    }

    #[test]
    fn test_two_packets_succeed() {
        let mut reorder = Reorder::new(2, 0, 1);
        // prepare data
        let (header, packet) = build_packet(MessageType::End, 0, 0);

        // must fail
        let ret = reorder.push(header, packet.clone());
        assert!(ret.is_none());

        // must succeed
        let (flags, session, block, packet) = reorder.push(header, packet).unwrap();

        // checks
        assert!(flags.contains(MessageType::End));
        assert_eq!(session, 0);
        assert_eq!(block, 0);
        assert_eq!(packet.len(), 2);
    }

    #[test]
    fn test_two_packets_two_blocks_succeed() {
        let mut reorder = Reorder::new(2, 0, 1);
        // prepare data
        let (header, packet) = build_packet(MessageType::Data, 0, 0);

        // must fail
        let ret = reorder.push(header, packet.clone());
        assert!(ret.is_none());

        // must succeed
        let (flags, session, block, packet) = reorder.push(header, packet).unwrap();

        // checks
        assert!(flags.contains(MessageType::Data));
        assert_eq!(session, 0);
        assert_eq!(block, 0);
        assert_eq!(packet.len(), 2);

        // prepare data
        let (header, packet) = build_packet(MessageType::End, 0, 1);

        // must fail
        let ret = reorder.push(header, packet.clone());
        assert!(ret.is_none());

        // must succeed
        let (flags, session, block, packet) = reorder.push(header, packet).unwrap();

        // checks
        assert!(flags.contains(MessageType::End));
        assert_eq!(session, 0);
        assert_eq!(block, 1);
        assert_eq!(packet.len(), 2);
    }

    #[test]
    fn test_one_packet_simple_block_reorder_succeed() {
        let mut reorder = Reorder::new(1, 0, 1);
        // prepare data
        let (header, packet) = build_packet(MessageType::End, 0, 1);

        // must fail
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());

        let (header, packet) = build_packet(MessageType::Data, 0, 0);

        // must succeed
        let (flags, session, block, packet) = reorder.push(header, packet).unwrap();

        // checks
        assert!(flags.contains(MessageType::Data));
        assert_eq!(session, 0);
        assert_eq!(block, 0);
        assert_eq!(packet.len(), 1);

        // XXX TODO we should try pop

        // must succeed
        let (flags, session, block, packet) = reorder.pop_first().unwrap();

        // checks
        assert!(flags.contains(MessageType::End));
        assert_eq!(session, 0);
        assert_eq!(block, 1);
        assert_eq!(packet.len(), 1);
    }

    #[test]
    fn test_one_packet_simple_session_reorder_succeed() {
        let mut reorder = Reorder::new(1, 0, 1);
        // prepare data
        let (header, packet) = build_packet(MessageType::End, 1, 0);

        // must fail
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());

        let (header, packet) = build_packet(MessageType::End, 0, 0);

        // must succeed
        let (flags, session, block, packet) = reorder.push(header, packet).unwrap();

        // checks
        assert!(flags.contains(MessageType::End));
        assert_eq!(session, 0);
        assert_eq!(block, 0);
        assert_eq!(packet.len(), 1);

        // XXX TODO we should try pop

        // must succeed
        let (flags, session, block, packet) = reorder.pop_first().unwrap();

        // checks
        assert!(flags.contains(MessageType::End));
        assert_eq!(session, 1);
        assert_eq!(block, 0);
        assert_eq!(packet.len(), 1);
    }

    #[test]
    fn test_two_packets_simple_session_reorder_succeed() {
        let mut reorder = Reorder::new(2, 0, 1);
        // prepare data
        let (header, packet) = build_packet(MessageType::End, 1, 0);

        // must fail
        let ret = reorder.push(header, packet.clone());
        assert!(ret.is_none());

        // prepare data

        // must fail
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());

        let (header, packet) = build_packet(MessageType::End, 0, 0);

        // must fail
        let ret = reorder.push(header, packet.clone());
        assert!(ret.is_none());

        // must succeed
        let (flags, session, block, packet) = reorder.push(header, packet).unwrap();

        // checks
        assert!(flags.contains(MessageType::End));
        assert_eq!(session, 0);
        assert_eq!(block, 0);
        assert_eq!(packet.len(), 2);

        // must succeed
        let (flags, session, block, packet) = reorder.pop_first().unwrap();

        // checks
        assert!(flags.contains(MessageType::End));
        assert_eq!(session, 1);
        assert_eq!(block, 0);
        assert_eq!(packet.len(), 2);
    }

    #[test]
    fn test_two_packets_two_blocks_simple_session_reorder_succeed() {
        let mut reorder = Reorder::new(2, 0, 1);
        // prepare data
        let (header, packet) = build_packet(MessageType::Data, 1, 0);

        // must fail
        let ret = reorder.push(header, packet.clone());
        assert!(ret.is_none());

        // must fail
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());

        // prepare data
        let (header, packet) = build_packet(MessageType::End, 1, 1);

        // must fail
        let ret = reorder.push(header, packet.clone());
        assert!(ret.is_none());

        // must fail
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());

        let (header, packet) = build_packet(MessageType::End, 0, 1);

        // must fail
        let ret = reorder.push(header, packet.clone());
        assert!(ret.is_none());

        // must fail
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());

        let (header, packet) = build_packet(MessageType::Data, 0, 0);

        // must fail
        let ret = reorder.push(header, packet.clone());
        assert!(ret.is_none());

        // must succeed
        let (flags, session, block, packet) = reorder.push(header, packet).unwrap();

        // checks
        assert!(flags.contains(MessageType::Data));
        assert_eq!(session, 0);
        assert_eq!(block, 0);
        assert_eq!(packet.len(), 2);

        // must succeed
        let (flags, session, block, packet) = reorder.pop_first().unwrap();

        // checks
        assert!(flags.contains(MessageType::End));
        assert_eq!(session, 0);
        assert_eq!(block, 1);
        assert_eq!(packet.len(), 2);

        // must succeed
        let (flags, session, block, packet) = reorder.pop_first().unwrap();

        // checks
        assert!(flags.contains(MessageType::Data));
        assert_eq!(session, 1);
        assert_eq!(block, 0);
        assert_eq!(packet.len(), 2);

        // must succeed
        let (flags, session, block, packet) = reorder.pop_first().unwrap();

        // checks
        assert!(flags.contains(MessageType::End));
        assert_eq!(session, 1);
        assert_eq!(block, 1);
        assert_eq!(packet.len(), 2);
    }

    #[test]
    fn test_lost_packet_timeout() {
        let mut reorder = Reorder::new(1, 1, 1);
        // prepare data
        let (header, packet) = build_packet(MessageType::End, 0, 0);

        // must fail
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());

        // wait for more than timeout
        std::thread::sleep(Duration::from_millis(MAX_DELAY_MS as u64 + 50));

        // must succeed
        let (flags, session, block, packet) = reorder.pop_first().unwrap();

        // checks
        assert!(flags.contains(MessageType::End));
        assert_eq!(session, 0);
        assert_eq!(block, 0);
        assert_eq!(packet.len(), 1);
    }

    #[test]
    fn test_unused_block_timeout_nothing() {
        let mut reorder = Reorder::new(1, 1, 1);

        // wait for more than timeout
        std::thread::sleep(Duration::from_millis(MAX_DELAY_MS as u64 + 50));

        // if no packet : there must be nothing returned
        let ret = reorder.pop_first();
        assert!(ret.is_none());
    }

    #[test]
    fn test_lost_packet_too_many_queues() {
        let mut reorder = Reorder::new(1, 1, 1);

        (0..MAX_ACTIVE_QUEUES).for_each(|i| {
            // prepare data
            let (header, packet) = build_packet(MessageType::Data, 0, i as u8);

            // must fail
            let ret = reorder.push(header, packet);
            assert!(ret.is_none());
        });

        let (header, packet) = build_packet(MessageType::Data, 0, MAX_ACTIVE_QUEUES as u8);

        // must fail
        let (flags, session, block, packets) = reorder.push(header, packet).unwrap();
        // checks
        assert!(flags.contains(MessageType::Data));
        assert_eq!(session, 0);
        assert_eq!(block, 0);
        assert_eq!(packets.len(), 1);
    }

    #[test]
    fn test_no_lost_packet_loop_not_too_many_queues() {
        let mut reorder = Reorder::new(1, 1, 1);

        // we do a full loop of 256 blocks
        (0..256).for_each(|i| {
            // prepare data
            let (header, packet) = build_packet(MessageType::Data, 0, i as u8);

            // must fail
            let ret = reorder.push(header, packet.clone());
            assert!(ret.is_none());

            // XXX strange
            let ret = reorder.push(header, packet);
            assert!(ret.is_some());
        });

        // now we add one more : must not be returned

        let (header, packet) = build_packet(MessageType::Data, 0, 0);

        // must fail
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());

        // must fail : no reason to force reblocking XXX ? but too many blocks => must return ?!
        let ret = reorder.pop_first();
        assert!(ret.is_none());
    }

    #[test]
    fn test_interrupt() {
        // check what appends when we loose many blocks
        // we push many packets without end then we start a new session that finish properly

        let mut reorder = Reorder::new(1, 0, 1);

        // we store many unfinished blocks
        (0..50).for_each(|i| {
            // prepare data
            let (header, packet) = build_packet(MessageType::Data, 0, i as u8);

            // must succeed
            let ret = reorder.push(header, packet);
            assert!(ret.is_some());
        });

        // we loose most of session 1 too (9 first blocks missing)
        let (header, packet) = build_packet(MessageType::End, 1, 10);
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());

        // now we add a small 3 blocks session in session 2
        let (header, packet) = build_packet(MessageType::Data | MessageType::Start, 2, 0);
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());

        let (header, packet) = build_packet(MessageType::Data, 2, 1);
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());

        let (header, packet) = build_packet(MessageType::End, 5, 3);
        let ret = reorder.push(header, packet);
        assert!(ret.is_none());

        // wait for more than timeout
        std::thread::sleep(Duration::from_millis(MAX_DELAY_MS as u64 + 50));

        // return probably session 0 first
        let ret = reorder.pop_first();
        assert!(ret.is_some());
    }

    #[test]
    fn test_session_loop() {
        let mut reorder = Reorder::new(1, 1, 1);

        // initialize with 256 completed sessions
        (0..256).for_each(|session| {
            // prepare data
            let (header, packet) =
                build_packet(MessageType::Start | MessageType::Data, session as u8, 0);

            // no finished
            let ret = reorder.push(header, packet);
            assert!(ret.is_none());

            let (header, packet) =
                build_packet(MessageType::Data | MessageType::End, session as u8, 0);

            // must succeed
            let ret = reorder.push(header, packet);
            assert!(ret.is_some());
        });

        // loop
        (1..10).for_each(|_loop| {
            // we store many unfinished blocks
            (0..256).for_each(|session| {
                // prepare data
                let (header, packet) =
                    build_packet(MessageType::Start | MessageType::Data, session as u8, 0);

                // no finished
                let ret = reorder.push(header, packet);
                assert!(ret.is_none());

                let (header, packet) =
                    build_packet(MessageType::Data | MessageType::End, session as u8, 0);

                // must succeed
                let ret = reorder.push(header, packet);
                assert!(ret.is_some());
            });
        });
    }
    // XXX TODO test multiple session (max active queue)
    // XXX TODO 10 sessions en parallèle
    // XXX TODO diode send / init
}
