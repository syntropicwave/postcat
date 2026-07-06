-- Pre-request and test scripts on requests, folders and collections.
-- Chain order at send time: collection -> folders (outer to inner) -> request.
ALTER TABLE collection_items ADD COLUMN pre_request_script TEXT;
ALTER TABLE collection_items ADD COLUMN test_script TEXT;
ALTER TABLE collections ADD COLUMN pre_request_script TEXT;
ALTER TABLE collections ADD COLUMN test_script TEXT;
