use tailwag_orm::{
    data_definition::table::Identifier,
    queries::filterable_types::{FilterableType, StringFilter, UuidFilter},
};

pub struct AccountFilterHelper {
    // TODO: Why not just implement for the types diirectly? e.g. "uuid::Uuid", "String", etc?
    pub id: FilterableType<UuidFilter>,
    pub email_address: FilterableType<StringFilter>,
}

impl Default for AccountFilterHelper {
    fn default() -> Self {
        Self {
            id: FilterableType::<UuidFilter>::new(Identifier::new_unchecked("id")),
            email_address: FilterableType::<StringFilter>::new(Identifier::new_unchecked(
                "email_address",
            )),
        }
    }
}
