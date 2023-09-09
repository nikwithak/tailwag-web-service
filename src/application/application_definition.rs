use tailwag_orm::database_definition::database_definition::DatabaseDefinition;

use super::WebServiceApplication;

#[derive(Debug)]
pub struct DataModelRestServiceDefinition {
    resources: DatabaseDefinition, // Owned - will build a webservice
    application: WebServiceApplication,
}

// impl Default for DataModelRestServiceDefinition {
// fn default() -> Self {
//     let db: DatabaseDefinition = DatabaseDefinition::new_unchecked("db").into();

//     DataModelRestServiceDefinition {
//         resources: db,
//         application: todo!(),
//     }
// }
// }


impl Into<WebServiceApplication> for DataModelRestServiceDefinition {
    fn into(self) -> WebServiceApplication {
        todo!()
    }
}
