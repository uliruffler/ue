// Tests for the "New" feature (Ctrl+N) that creates untitled files
use serial_test::serial;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn setup_test_env() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    unsafe {
        std::env::set_var("UE_TEST_HOME", temp_dir.path());
    }
    temp_dir
}

#[test]
#[serial]
fn test_untitled_file_is_created_on_disk() {
    let temp_home = setup_test_env();
    let home_dir = temp_home.path();
    
    // Simulate creating an untitled file by saving undo history for "untitled"
    let undo_history = ue::undo::UndoHistory::new();
    undo_history.save("untitled").unwrap();
    
    // Verify the undo file was created in the correct location
    let expected_path = home_dir.join(".ue/files/untitled.ue");
    assert!(expected_path.exists(), "Untitled undo file should be created at {}", expected_path.display());
}

#[test]
#[serial]
fn test_untitled_cleanup_after_save() {
    let temp_home = setup_test_env();
    let home_dir = temp_home.path();
    
    // Create an untitled undo file
    let mut undo_history = ue::undo::UndoHistory::new();
    undo_history.push(ue::undo::Edit::InsertChar { line: 0, col: 0, ch: 'a' });
    undo_history.save("untitled").unwrap();
    
    let untitled_path = home_dir.join(".ue/files/untitled.ue");
    assert!(untitled_path.exists(), "Untitled undo file should exist before save");
    
    // Simulate saving untitled to a real filename
    let real_file = home_dir.join("myfile.txt");
    fs::write(&real_file, "test content").unwrap();
    
    // Delete the untitled history
    ue::editing::delete_file_history("untitled").unwrap();
    
    // Verify untitled.ue was deleted
    assert!(!untitled_path.exists(), "Untitled undo file should be deleted after save");
    
    // Create undo file for the real filename
    undo_history.save(real_file.to_str().unwrap()).unwrap();
    
    // Verify the real file's undo history exists
    let real_undo_path = home_dir.join(format!(".ue/files/{}/myfile.txt.ue", home_dir.to_str().unwrap().trim_start_matches('/')));
    assert!(real_undo_path.exists(), "Real file undo history should exist at {}", real_undo_path.display());
}

#[test]
#[serial]
fn test_multiple_untitled_files() {
    let temp_home = setup_test_env();
    let home_dir = temp_home.path();
    
    // Create multiple untitled files
    let undo1 = ue::undo::UndoHistory::new();
    undo1.save("untitled").unwrap();
    
    let undo2 = ue::undo::UndoHistory::new();
    undo2.save("untitled-2").unwrap();
    
    let undo3 = ue::undo::UndoHistory::new();
    undo3.save("untitled-3").unwrap();
    
    // Verify all were created
    assert!(home_dir.join(".ue/files/untitled.ue").exists());
    assert!(home_dir.join(".ue/files/untitled-2.ue").exists());
    assert!(home_dir.join(".ue/files/untitled-3.ue").exists());
    
    // Cleanup one of them
    ue::editing::delete_file_history("untitled-2").unwrap();
    
    // Verify only the specified one was deleted
    assert!(home_dir.join(".ue/files/untitled.ue").exists(), "untitled should still exist");
    assert!(!home_dir.join(".ue/files/untitled-2.ue").exists(), "untitled-2 should be deleted");
    assert!(home_dir.join(".ue/files/untitled-3.ue").exists(), "untitled-3 should still exist");
}

#[test]
#[serial]
fn test_untitled_not_saved_to_subdirectory() {
    let temp_home = setup_test_env();
    let home_dir = temp_home.path();
    
    // Save an untitled file
    let undo = ue::undo::UndoHistory::new();
    undo.save("untitled").unwrap();
    
    // Verify it's in the root of .ue/files/, not in a subdirectory
    let expected_path = home_dir.join(".ue/files/untitled.ue");
    assert!(expected_path.exists(), "Untitled should be in .ue/files/ root");
    
    // Make sure it's not in a subdirectory like .ue/files/untitled/untitled.ue
    let wrong_path = home_dir.join(".ue/files/untitled/untitled.ue");
    assert!(!wrong_path.exists(), "Untitled should NOT be in a subdirectory");
}

#[test]
#[serial]
fn test_untitled_case_insensitive() {
    let temp_home = setup_test_env();
    let home_dir = temp_home.path();
    
    // Test that UNTITLED (uppercase) is also treated as untitled
    let undo = ue::undo::UndoHistory::new();
    undo.save("UNTITLED").unwrap();
    
    let expected_path = home_dir.join(".ue/files/UNTITLED.ue");
    assert!(expected_path.exists(), "UNTITLED (uppercase) should be created in .ue/files/ root");
}

#[test]
#[serial]
fn test_untitled_with_path_is_not_untitled() {
    let temp_home = setup_test_env();
    let home_dir = temp_home.path();
    
    // A file path containing "untitled" but not as a simple filename should NOT be treated as untitled
    let test_file = home_dir.join("untitled").join("document.txt");
    fs::create_dir_all(test_file.parent().unwrap()).unwrap();
    fs::write(&test_file, "content").unwrap();
    
    let undo = ue::undo::UndoHistory::new();
    undo.save(test_file.to_str().unwrap()).unwrap();
    
    // Should be in a subdirectory structure, not in .ue/files/ root
    let untitled_root_path = home_dir.join(".ue/files/document.txt.ue");
    assert!(!untitled_root_path.exists(), "Path with 'untitled' dir should not be in root");
    
    // Should be in the proper subdirectory
    let proper_path_str = test_file.to_str().unwrap().trim_start_matches('/');
    let proper_path = home_dir.join(format!(".ue/files/{}", proper_path_str))
        .parent().unwrap()
        .join("document.txt.ue");
    assert!(proper_path.exists(), "Should be in proper subdirectory structure");
}

#[test]
#[serial]
fn test_untitled_removed_from_recent_files_after_save() {
    let _temp_home = setup_test_env();
    
    // Add untitled to recent files
    ue::recent::update_recent_file("untitled").unwrap();
    
    let recent_before: Vec<PathBuf> = ue::recent::get_recent_files().unwrap();
    let has_untitled_before = recent_before.iter().any(|p| {
        p.to_str().map(|s| s.contains("untitled")).unwrap_or(false)
    });
    assert!(has_untitled_before, "Untitled should be in recent files initially");
    
    // Remove untitled from recent files (simulates save to real filename)
    ue::recent::remove_recent_file("untitled").unwrap();
    
    let recent_after: Vec<PathBuf> = ue::recent::get_recent_files().unwrap();
    let has_untitled_after = recent_after.iter().any(|p| {
        p.to_str().map(|s| s.contains("untitled")).unwrap_or(false)
    });
    assert!(!has_untitled_after, "Untitled should be removed from recent files after save");
}

