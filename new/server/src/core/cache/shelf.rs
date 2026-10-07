use std::hash::{BuildHasher, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use foldhash::fast::FixedState;

use crate::core::error::{AppFail, AppResult};
use super::arch::{Ledger, Shelf, Slot};

const PREFIX: usize = 12;

impl Shelf {

    pub fn open ( root: &Path, capacity: u64, now_ms: u64 ) -> AppResult<Self> {

        std::fs::create_dir_all(root).or_fail_with(|| format!("cannot create cache directory {}", root.display()))?;

        let mut ledger = Ledger::default();

        for shard in std::fs::read_dir(root).or_fail_with(|| format!("cannot list cache directory {}", root.display()))?.flatten().filter(|shard| shard.path().is_dir()) {

            for file in std::fs::read_dir(shard.path()).into_iter().flatten().flatten() {

                let path = file.path();
                let id = path.file_name().and_then(|name| name.to_str()).and_then(|name| u128::from_str_radix(name, 16).ok());
                let mut head = [0u8; 8];
                let fresh = std::fs::File::open(&path).and_then(|mut handle| handle.read_exact(&mut head)).is_ok() && u64::from_le_bytes(head) > now_ms;

                match ( id, fresh, file.metadata() ) {
                    ( Some(id), true, Ok(meta) ) => { ledger.bytes += meta.len(); ledger.slots.insert(id, Slot { size: meta.len(), expires_ms: u64::from_le_bytes(head), used: 0 }); }
                    _ => { let _ = std::fs::remove_file(&path); }
                }

            }

        }

        Ok(Self { root: root.to_path_buf(), capacity, ledger: Mutex::new(ledger) })

    }

    pub fn holds ( &self, key: &[u8], now_ms: u64 ) -> bool {

        self.ledger.lock().is_ok_and(|ledger| ledger.slots.get(&Self::id(key)).is_some_and(|slot| slot.expires_ms > now_ms))

    }

    pub fn get ( &self, key: &[u8], now_ms: u64 ) -> Option<( u64, Vec<u8> )> {

        let id = Self::id(key);

        let expires_ms = {

            let mut ledger = self.ledger.lock().ok()?;

            ledger.tick += 1;

            let tick = ledger.tick;
            let slot = ledger.slots.get_mut(&id).filter(|slot| slot.expires_ms > now_ms)?;

            slot.used = tick;
            slot.expires_ms

        };

        let mut data = std::fs::read(self.path(id)).ok()?;
        let length = u32::from_le_bytes(data.get(8..PREFIX)?.try_into().ok()?) as usize;

        if data.get(PREFIX..PREFIX + length)? != key { return None; }

        Some(( expires_ms, data.split_off(PREFIX + length) ))

    }

    pub fn put ( &self, key: &[u8], expires_ms: u64, payload: &[u8] ) -> bool {

        let id = Self::id(key);
        let path = self.path(id);
        let draft = path.with_extension("part");
        let size = (PREFIX + key.len() + payload.len()) as u64;

        if size > self.capacity { return false; }

        let written = path.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::File::create(&draft)).and_then(|mut file| {

            file.write_all(&expires_ms.to_le_bytes())?;
            file.write_all(&(key.len() as u32).to_le_bytes())?;
            file.write_all(key)?;
            file.write_all(payload)

        }).and_then(|_| std::fs::rename(&draft, &path));

        if written.is_err() { let _ = std::fs::remove_file(&draft); return false; }

        let Ok(mut ledger) = self.ledger.lock() else { return false; };

        ledger.tick += 1;

        let tick = ledger.tick;

        if let Some(old) = ledger.slots.insert(id, Slot { size, expires_ms, used: tick }) { ledger.bytes -= old.size.min(ledger.bytes); }

        ledger.bytes += size;

        if ledger.bytes > self.capacity {

            let mut order: Vec<( u64, u128 )> = ledger.slots.iter().filter(|( other, _ )| **other != id).map(|( other, slot )| ( slot.used, *other )).collect();

            order.sort_unstable();

            for ( _, victim ) in order {

                if ledger.bytes <= self.capacity / 10 * 9 { break; }

                if let Some(slot) = ledger.slots.remove(&victim) { ledger.bytes -= slot.size.min(ledger.bytes); let _ = std::fs::remove_file(self.path(victim)); }

            }

        }

        true

    }

    pub fn remove ( &self, key: &[u8] ) -> bool {

        let id = Self::id(key);
        let Ok(mut ledger) = self.ledger.lock() else { return false; };
        let Some(slot) = ledger.slots.remove(&id) else { return false; };

        ledger.bytes -= slot.size.min(ledger.bytes);

        std::fs::remove_file(self.path(id)).is_ok()

    }

    pub fn clear ( &self ) {

        let Ok(mut ledger) = self.ledger.lock() else { return; };

        for id in ledger.slots.keys() { let _ = std::fs::remove_file(self.path(*id)); }

        ledger.slots.clear();
        ledger.bytes = 0;

    }

    pub fn len ( &self ) -> usize {

        self.ledger.lock().map_or(0, |ledger| ledger.slots.len())

    }

    pub fn is_empty ( &self ) -> bool {

        self.len() == 0

    }

    pub fn bytes ( &self ) -> u64 {

        self.ledger.lock().map_or(0, |ledger| ledger.bytes)

    }

    fn id ( key: &[u8] ) -> u128 {

        let half = |seed: u64| { let mut hasher = FixedState::with_seed(seed).build_hasher(); hasher.write(key); hasher.finish() };

        (u128::from(half(0x51ab_3c90_77e1_42d7)) << 64) | u128::from(half(0x0f6e_91b4_2ad8_c563))

    }

    fn path ( &self, id: u128 ) -> PathBuf {

        self.root.join(format!("{:02x}", id as u8)).join(format!("{id:032x}"))

    }

}
