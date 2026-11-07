use misaka_palw_kernel::route::{KernelRouteObjectV1 as Route, borrowed_native_action_v1};
#[test]
fn action80_owned_and_borrowed_frames_have_the_same_published_bound() {
    for size in [0, 64, (32 << 10) + 256] {
        let object = Route::StageExecutionV14 { action: 80, body: vec![0; size] };
        let raw = object.encode();
        assert_eq!(Route::decode(&raw).unwrap(), object);
        let frame = borrowed_native_action_v1(&raw).unwrap().unwrap();
        assert_eq!(frame.action, 80);
        assert_eq!(frame.body.len(), size);
    }
    for action in [80, 81] {
        let raw = Route::StageExecutionV14 { action, body: vec![0; (32 << 10) + 257] }.encode();
        assert!(Route::decode(&raw).is_err());
        assert!(borrowed_native_action_v1(&raw).is_err());
    }
    let raw = Route::StageExecutionV14 { action: 81, body: vec![] }.encode();
    assert!(Route::decode(&raw).is_err());
    assert!(borrowed_native_action_v1(&raw).is_err());
}
