use indicatif::{ProgressBar, ProgressStyle};

/// Creates and configures a new `ProgressBar` with a custom style and optional message.
pub fn create_progress_bar(total: usize, msg: Option<String>) -> ProgressBar {
    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::with_template("{spinner:.green} [{wide_bar}] {percent}% ({msg})")
            .unwrap()
            .progress_chars("█▓▒░")
            .tick_chars("⠋⠙⠚⠉"),
    );
    if let Some(m) = msg {
        pb.set_message(m);
    } else {
        pb.set_message("Processing".to_string());
    }
    pb
}

/// Computes the Levenshtein distance between two strings.
pub fn levenshtein_dist(s1: &str, s2: &str) -> usize {
    let len1 = s1.chars().count();
    let len2 = s2.chars().count();
    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    for i in 0..=len1 {
        matrix[i][0] = i;
    }
    for j in 0..=len2 {
        matrix[0][j] = j;
    }

    s1.chars().enumerate().for_each(|(i, c1)| {
        s2.chars().enumerate().for_each(|(j, c2)| {
            let cost = if c1 == c2 { 0 } else { 1 };
            matrix[i + 1][j + 1] = std::cmp::min(
                matrix[i][j + 1] + 1,
                std::cmp::min(matrix[i + 1][j] + 1, matrix[i][j] + cost),
            );
        });
    });

    return matrix[len1][len2];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_dist() {
        assert_eq!(levenshtein_dist("kitten", "sitting"), 3);
        assert_eq!(levenshtein_dist("flaw", "lawn"), 2);
        assert_eq!(levenshtein_dist("intention", "execution"), 5);
        assert_eq!(levenshtein_dist("", ""), 0);
        assert_eq!(levenshtein_dist("abc", "abc"), 0);
    }
}
