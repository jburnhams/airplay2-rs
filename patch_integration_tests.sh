sed -i 's/let cmd = coord.calculate_adjustment();/let cmd = coord.calculate_adjustment_at(now_ptp);/' tests/multi_room_integration.rs
sed -i 's/let cmd = coord.calculate_adjustment();/let cmd = coord.calculate_adjustment_at(now_ptp);/' src/receiver/ap2/tests/multi_room.rs
