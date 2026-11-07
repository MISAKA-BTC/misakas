while materials.len() > PANEL_POOL_MAX_CLAIMS || pool_bytes > PANEL_POOL_MAX_BYTES {
                            // Never evict the claim that just arrived: that would make the pool
                            // drop exactly the payload it was asked to hold, which is the M2-1
                            // failure in a different costume.
                            let Some(oldest) =
                                pool_arrival.iter().filter(|(id, _)| **id != claim).min_by_key(|(_, seq)| **seq).map(|(id, _)| *id)
                            else {
                                break;
                            };
                            if let Some(evicted) = materials.remove(&oldest) {
                                pool_bytes = pool_bytes.saturating_sub(evicted.iter().map(|b| b.len()).sum::<usize>());
                            }
                            pool_arrival.remove(&oldest);
                            first_seen.remove(&oldest);
                        }