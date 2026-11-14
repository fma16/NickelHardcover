use anyhow::{Context, Result};
use argh::FromArgs;
use rusqlite::{Connection, OpenFlags};

use crate::appcontext::AppContext;
use crate::config::VERSION;
use crate::messages::{Annotation, AnnotationList, Messages};
use crate::utils::send_msg;

/// List aggregated bookmarks.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "list-bookmarks")]
pub struct ListBookmarks {}

pub fn run(context: &mut AppContext, args: &ListBookmarks) -> Result<()> {
  log::info!("{VERSION} {args:?}");

  let connection =
    Connection::open_with_flags(&context.config.sqlite_path, OpenFlags::SQLITE_OPEN_READ_ONLY).context(format!(
      "Failed to connect to the database <i>{}</i>",
      context.config.sqlite_path
    ))?;
  let annotations = list_annotations(&connection)?;

  if annotations.is_empty() {
    return Ok(());
  }

  send_msg!(&Messages::AnnotationList(AnnotationList { annotations }))
}

fn list_annotations(connection: &Connection) -> Result<Vec<Annotation>> {
  connection
    .prepare(
      "SELECT
        Title,
        Attribution,
        VolumeID,
        COUNT(DISTINCT BookmarkID),
        MAX(Bookmark.DateModified)
      FROM Bookmark
      INNER JOIN content
        ON content.ContentId = VolumeID AND BookTitle is null
      WHERE Hidden = 'false'
      AND bookmark.Text != ''
      GROUP BY VolumeID
      ORDER BY Bookmark.DateModified DESC;",
    )
    .context("Failed to prepare list bookmarks query")?
    .query_map([], |row| {
      Ok(Annotation {
        title: row.get::<_, String>(0)?,
        attribution: row.get::<_, String>(1)?,
        volume_id: row.get::<_, String>(2)?,
        count: row.get::<_, u32>(3)?,
        last_modified: row.get::<_, String>(4)?,
      })
    })
    .context("Failed to run list bookmarks query")?
    .collect::<Result<Vec<_>, _>>()
    .context("Failed to map list bookmarks query result")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn orphan_bookmarks_do_not_hide_available_books() {
    let connection = Connection::open_in_memory().unwrap();
    connection
      .execute_batch(
        "\
      CREATE TABLE content (ContentId TEXT, Title TEXT, Attribution TEXT, BookTitle TEXT);
      CREATE TABLE Bookmark (BookmarkID TEXT, VolumeID TEXT, Text TEXT, Hidden TEXT, DateModified TEXT);
      INSERT INTO content VALUES ('book', 'Available book', 'Author', NULL);
      INSERT INTO Bookmark VALUES ('1', 'book', 'Quote', 'false', '2026-11-10');
      INSERT INTO Bookmark VALUES ('2', 'book', 'Other quote', 'false', '2026-11-12');
      INSERT INTO Bookmark VALUES ('3', 'book', 'Hidden quote', 'true', '2026-11-13');
      INSERT INTO Bookmark VALUES ('4', 'book', '', 'false', '2026-11-13');
      INSERT INTO Bookmark VALUES ('5', 'deleted-book', 'Orphan quote', 'false', '2026-11-13');
    ",
      )
      .unwrap();
    let annotations = list_annotations(&connection).unwrap();
    assert_eq!(annotations.len(), 1);
    let annotation = &annotations[0];
    assert_eq!(annotation.title, "Available book");
    assert_eq!(annotation.attribution, "Author");
    assert_eq!(annotation.volume_id, "book");
    assert_eq!(annotation.count, 2);
    assert_eq!(annotation.last_modified, "2026-11-12");
    connection.execute("DELETE FROM content", []).unwrap();
    assert!(list_annotations(&connection).unwrap().is_empty());
    assert_eq!(
      connection
        .query_row("SELECT COUNT(*) FROM Bookmark", [], |row| row.get::<_, u32>(0))
        .unwrap(),
      5
    );
  }
}
