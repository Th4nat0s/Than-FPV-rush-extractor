//! The schema, migration for migration as GRDB applied it — the DDL below is
//! what GRDB 7 generated for the Swift migrator, read back from a real library.
//!
//! The ledger is GRDB's own `grdb_migrations` table, kept so a library the
//! macOS app migrated is recognised as up to date. A migration that has
//! shipped is frozen: a real library already ran it, and an edited migration
//! does not re-run. Every schema change is a new entry at the end.
//!
//! The schema keeps `flight` and `clip` as separate tables on purpose: the DVR
//! cuts a recording mid-flight and the feed continues in the next file, so a
//! flight owns one clip per file it spans (CLAUDE.md 3.3).

use rusqlite::{Connection, OptionalExtension};

pub(crate) const MIGRATIONS: &[(&str, &str)] = &[
    ("v1", r#"
        CREATE TABLE "session" ("id" TEXT PRIMARY KEY NOT NULL, "name" TEXT NOT NULL, "date" DATETIME, "location" TEXT, "notes" TEXT);
        CREATE TABLE "sourceFile" ("id" TEXT PRIMARY KEY NOT NULL, "sessionId" TEXT NOT NULL REFERENCES "session"("id") ON DELETE CASCADE, "originalPath" TEXT NOT NULL, "importedPath" TEXT NOT NULL UNIQUE, "sequenceNumber" INTEGER, "width" INTEGER NOT NULL, "height" INTEGER NOT NULL, "frameRateNumerator" INTEGER NOT NULL, "frameRateDenominator" INTEGER NOT NULL, "frameCount" INTEGER, "duration" DOUBLE NOT NULL, "videoCodec" TEXT NOT NULL, "audioCodec" TEXT, "creationDate" DATETIME, "checksum" TEXT);
        CREATE INDEX "sourceFile_on_sessionId" ON "sourceFile"("sessionId");
        CREATE TABLE "flight" ("id" TEXT PRIMARY KEY NOT NULL, "sessionId" TEXT NOT NULL REFERENCES "session"("id") ON DELETE CASCADE, "orderInSession" INTEGER NOT NULL, "title" TEXT, "rating" INTEGER);
        CREATE INDEX "flight_on_sessionId" ON "flight"("sessionId");
        CREATE TABLE "clip" ("id" TEXT PRIMARY KEY NOT NULL, "flightId" TEXT NOT NULL REFERENCES "flight"("id") ON DELETE CASCADE, "sourceFileId" TEXT NOT NULL REFERENCES "sourceFile"("id") ON DELETE CASCADE, "startFrame" INTEGER NOT NULL, "endFrame" INTEGER NOT NULL, "kind" TEXT NOT NULL, "entry" TEXT NOT NULL, "exit" TEXT NOT NULL, "confidence" DOUBLE NOT NULL, "craftName" TEXT, "thumbnailPath" TEXT);
        CREATE INDEX "clip_on_flightId" ON "clip"("flightId");
        CREATE INDEX "clip_on_sourceFileId" ON "clip"("sourceFileId");
        CREATE TABLE "detectionRun" ("id" TEXT PRIMARY KEY NOT NULL, "sourceFileId" TEXT NOT NULL REFERENCES "sourceFile"("id") ON DELETE CASCADE, "algorithmVersion" TEXT NOT NULL, "parameters" TEXT NOT NULL, "ranAt" DATETIME NOT NULL);
        CREATE INDEX "detectionRun_on_sourceFileId" ON "detectionRun"("sourceFileId");
    "#),
    // Figure tags hang off the recording, not the clip: re-segmenting deletes
    // every clip of a file, and a cascade from there would take the user's tags
    // with it. The proxy cache's access time lives here because atime is
    // commonly off.
    ("v2", r#"
        CREATE TABLE "figureTag" ("id" TEXT PRIMARY KEY NOT NULL, "sourceFileId" TEXT NOT NULL REFERENCES "sourceFile"("id") ON DELETE CASCADE, "startFrame" INTEGER NOT NULL, "endFrame" INTEGER NOT NULL, "label" TEXT NOT NULL, "note" TEXT, "createdAt" DATETIME NOT NULL);
        CREATE INDEX "figureTag_on_sourceFileId" ON "figureTag"("sourceFileId");
        CREATE TABLE "proxy" ("key" TEXT PRIMARY KEY NOT NULL, "bytes" INTEGER NOT NULL, "createdAt" DATETIME NOT NULL, "lastUsedAt" DATETIME NOT NULL);
    "#),
];

/// Applies every migration not yet in the ledger, up to and including
/// `up_to` (all of them when `None`), each in its own transaction.
pub(crate) fn migrate(connection: &mut Connection, up_to: Option<&str>) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS grdb_migrations (identifier TEXT NOT NULL PRIMARY KEY)")?;
    for &(identifier, sql) in MIGRATIONS {
        let applied = connection
            .query_row("SELECT 1 FROM grdb_migrations WHERE identifier = ?", [identifier],
                       |_| Ok(()))
            .optional()?
            .is_some();
        if !applied {
            let transaction = connection.transaction()?;
            transaction.execute_batch(sql)?;
            transaction.execute("INSERT INTO grdb_migrations (identifier) VALUES (?)",
                                [identifier])?;
            transaction.commit()?;
        }
        if up_to == Some(identifier) {
            break;
        }
    }
    Ok(())
}
