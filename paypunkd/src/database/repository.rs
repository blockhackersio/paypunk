use rusqlite::Connection;

pub trait Repository<T> {
    fn save(&self, conn: &Connection, entity: &T) -> Result<(), String>;
    fn find_all(&self, conn: &Connection) -> Result<Vec<T>, String>;
}
