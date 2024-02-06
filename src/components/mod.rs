pub enum ApplicationComponent {
    Service,
    Route,
    DataSource,
}

pub struct PostgresDataSourceConfig {
    conn_string: String,
    max_connections: u32,
}
