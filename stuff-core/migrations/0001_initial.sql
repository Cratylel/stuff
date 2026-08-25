-- Canonical relational state for a fresh pre-v1 database.
CREATE TABLE state_revision (
    id smallint PRIMARY KEY CHECK (id = 1),
    revision bigint NOT NULL CHECK (revision >= 0),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
INSERT INTO state_revision (id, revision) VALUES (1, 0);

CREATE TABLE storage_nodes (
    id uuid PRIMARY KEY,
    name text NOT NULL CHECK (btrim(name) <> ''),
    kind text NOT NULL CHECK (kind IN
        ('room', 'rack', 'shelf', 'drawer', 'fixture', 'cell_less_organizer', 'organizer', 'box', 'bin', 'loose', 'custom')),
    custom_kind text,
    parent_id uuid REFERENCES storage_nodes(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
    position_x bigint,
    position_y bigint,
    position_z bigint,
    width bigint,
    depth bigint,
    height bigint,
    metadata jsonb NOT NULL DEFAULT 'null'::jsonb,
    CHECK ((position_x IS NULL AND position_y IS NULL AND position_z IS NULL)
        OR (position_x IS NOT NULL AND position_y IS NOT NULL AND position_z IS NOT NULL)),
    CHECK ((width IS NULL AND depth IS NULL AND height IS NULL)
        OR (width IS NOT NULL AND depth IS NOT NULL AND height IS NOT NULL)),
    CHECK (width IS NULL OR (width > 0 AND depth > 0 AND height > 0)),
    CHECK ((kind = 'custom' AND custom_kind IS NOT NULL AND btrim(custom_kind) <> '')
        OR (kind <> 'custom' AND custom_kind IS NULL))
);
CREATE INDEX storage_nodes_parent_idx ON storage_nodes(parent_id);
CREATE INDEX storage_nodes_name_lower_idx ON storage_nodes(lower(name));

CREATE TABLE organizer_templates (
    id uuid PRIMARY KEY,
    name text NOT NULL CHECK (btrim(name) <> ''),
    width bigint NOT NULL CHECK (width > 0),
    depth bigint NOT NULL CHECK (depth > 0),
    height bigint NOT NULL CHECK (height > 0),
    metadata jsonb NOT NULL DEFAULT 'null'::jsonb
);
CREATE TABLE organizer_template_cells (
    template_id uuid NOT NULL REFERENCES organizer_templates(id) ON DELETE CASCADE,
    row_no bigint NOT NULL CHECK (row_no >= 0),
    column_no bigint NOT NULL CHECK (column_no >= 0),
    x bigint NOT NULL,
    y bigint NOT NULL,
    width bigint NOT NULL CHECK (width > 0),
    depth bigint NOT NULL CHECK (depth > 0),
    PRIMARY KEY (template_id, row_no, column_no)
);
CREATE TABLE organizer_template_dividers (
    template_id uuid NOT NULL,
    a_row bigint NOT NULL CHECK (a_row >= 0),
    a_column bigint NOT NULL CHECK (a_column >= 0),
    b_row bigint NOT NULL CHECK (b_row >= 0),
    b_column bigint NOT NULL CHECK (b_column >= 0),
    axis text NOT NULL CHECK (axis IN ('vertical', 'horizontal')),
    kind text NOT NULL CHECK (kind IN ('fixed', 'removable', 'impossible')),
    CHECK (ROW(a_row, a_column) < ROW(b_row, b_column)),
    PRIMARY KEY (template_id, a_row, a_column, b_row, b_column),
    FOREIGN KEY (template_id, a_row, a_column)
        REFERENCES organizer_template_cells(template_id, row_no, column_no) ON DELETE CASCADE,
    FOREIGN KEY (template_id, b_row, b_column)
        REFERENCES organizer_template_cells(template_id, row_no, column_no) ON DELETE CASCADE
);
CREATE TABLE organizer_instances (
    id uuid PRIMARY KEY,
    node_id uuid NOT NULL UNIQUE REFERENCES storage_nodes(id) ON DELETE CASCADE,
    template_id uuid NOT NULL REFERENCES organizer_templates(id) ON DELETE RESTRICT,
    UNIQUE (id, node_id)
);
CREATE INDEX organizer_instances_template_idx ON organizer_instances(template_id);
CREATE TABLE organizer_removed_dividers (
    organizer_id uuid NOT NULL REFERENCES organizer_instances(id) ON DELETE CASCADE,
    divider_key text NOT NULL CHECK (btrim(divider_key) <> ''),
    PRIMARY KEY (organizer_id, divider_key)
);

CREATE TABLE rack_templates (
    id uuid PRIMARY KEY,
    name text NOT NULL CHECK (btrim(name) <> ''),
    width bigint NOT NULL CHECK (width > 0),
    depth bigint NOT NULL CHECK (depth > 0),
    height bigint NOT NULL CHECK (height > 0),
    metadata jsonb NOT NULL DEFAULT 'null'::jsonb
);
CREATE TABLE rack_template_shelves (
    template_id uuid NOT NULL REFERENCES rack_templates(id) ON DELETE CASCADE,
    ordinal integer NOT NULL CHECK (ordinal >= 0),
    name text NOT NULL CHECK (btrim(name) <> ''),
    altitude bigint NOT NULL CHECK (altitude >= 0),
    usable_height bigint NOT NULL CHECK (usable_height > 0),
    metadata jsonb NOT NULL DEFAULT 'null'::jsonb,
    PRIMARY KEY (template_id, ordinal),
    UNIQUE (template_id, name)
);
CREATE TABLE rack_instances (
    rack_id uuid PRIMARY KEY REFERENCES storage_nodes(id) ON DELETE CASCADE,
    template_id uuid NOT NULL REFERENCES rack_templates(id) ON DELETE RESTRICT
);
CREATE INDEX rack_instances_template_idx ON rack_instances(template_id);
CREATE TABLE rack_instance_shelves (
    rack_id uuid NOT NULL REFERENCES rack_instances(rack_id) ON DELETE CASCADE,
    ordinal integer NOT NULL CHECK (ordinal >= 0),
    shelf_id uuid NOT NULL UNIQUE REFERENCES storage_nodes(id) ON DELETE CASCADE,
    PRIMARY KEY (rack_id, ordinal)
);

-- A cabinet is a physical node; its generated Drawer children keep stable IDs.
CREATE TABLE drawer_cabinet_templates (
    id uuid PRIMARY KEY,
    name text NOT NULL CHECK (btrim(name) <> ''),
    width bigint NOT NULL CHECK (width > 0),
    depth bigint NOT NULL CHECK (depth > 0),
    height bigint NOT NULL CHECK (height > 0),
    metadata jsonb NOT NULL DEFAULT 'null'::jsonb
);
CREATE TABLE drawer_template_drawers (
    template_id uuid NOT NULL REFERENCES drawer_cabinet_templates(id) ON DELETE CASCADE,
    ordinal integer NOT NULL CHECK (ordinal >= 0),
    name text NOT NULL CHECK (btrim(name) <> ''),
    altitude bigint NOT NULL CHECK (altitude >= 0),
    usable_width bigint NOT NULL CHECK (usable_width > 0),
    usable_depth bigint NOT NULL CHECK (usable_depth > 0),
    usable_height bigint NOT NULL CHECK (usable_height > 0),
    metadata jsonb NOT NULL DEFAULT 'null'::jsonb,
    PRIMARY KEY (template_id, ordinal),
    UNIQUE (template_id, name)
);
CREATE TABLE drawer_cabinet_instances (
    cabinet_id uuid PRIMARY KEY REFERENCES storage_nodes(id) ON DELETE CASCADE,
    template_id uuid NOT NULL REFERENCES drawer_cabinet_templates(id) ON DELETE RESTRICT
);
CREATE INDEX drawer_cabinet_instances_template_idx ON drawer_cabinet_instances(template_id);
CREATE TABLE drawer_instance_nodes (
    cabinet_id uuid NOT NULL REFERENCES drawer_cabinet_instances(cabinet_id) ON DELETE CASCADE,
    ordinal integer NOT NULL CHECK (ordinal >= 0),
    drawer_id uuid NOT NULL UNIQUE REFERENCES storage_nodes(id) ON DELETE CASCADE,
    PRIMARY KEY (cabinet_id, ordinal)
);

CREATE TABLE items (
    id uuid PRIMARY KEY,
    name text NOT NULL CHECK (btrim(name) <> ''),
    manufacturer text,
    manufacturer_part_number text,
    category text,
    notes text,
    item_width bigint,
    item_depth bigint,
    item_height bigint,
    metadata jsonb NOT NULL DEFAULT 'null'::jsonb,
    CHECK ((item_width IS NULL AND item_depth IS NULL AND item_height IS NULL)
        OR (item_width > 0 AND item_depth > 0 AND item_height > 0))
);
CREATE INDEX items_name_lower_idx ON items(lower(name));
CREATE INDEX items_part_number_lower_idx ON items(lower(manufacturer_part_number))
    WHERE manufacturer_part_number IS NOT NULL;
CREATE TABLE item_tags (
    item_id uuid NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    ordinal integer NOT NULL CHECK (ordinal >= 0),
    tag text NOT NULL,
    PRIMARY KEY (item_id, ordinal)
);
CREATE INDEX item_tags_tag_lower_idx ON item_tags(lower(tag));

-- Locations are shared by stock, reservations, and occupancy. Cell-less
-- organizer nodes and ordinary containers use the node-only form; typed
-- organizer cells use a compartment.
CREATE TABLE inventory_locations (
    id uuid PRIMARY KEY,
    node_id uuid NOT NULL REFERENCES storage_nodes(id) ON DELETE RESTRICT,
    organizer_id uuid,
    compartment_key text,
    FOREIGN KEY (organizer_id, node_id)
        REFERENCES organizer_instances(id, node_id) ON DELETE RESTRICT,
    CHECK ((organizer_id IS NULL AND compartment_key IS NULL)
        OR (organizer_id IS NOT NULL AND compartment_key IS NOT NULL AND btrim(compartment_key) <> ''))
);
CREATE UNIQUE INDEX inventory_locations_node_only_uidx
    ON inventory_locations(node_id) WHERE organizer_id IS NULL;
CREATE UNIQUE INDEX inventory_locations_compartment_uidx
    ON inventory_locations(node_id, organizer_id, compartment_key) WHERE organizer_id IS NOT NULL;
CREATE INDEX inventory_locations_organizer_idx ON inventory_locations(organizer_id);
CREATE TABLE stock_lots (
    id uuid PRIMARY KEY,
    item_id uuid NOT NULL REFERENCES items(id) ON DELETE RESTRICT,
    location_id uuid NOT NULL REFERENCES inventory_locations(id) ON DELETE RESTRICT,
    quantity bigint NOT NULL CHECK (quantity > 0),
    position_x bigint,
    position_y bigint,
    position_z bigint,
    CHECK ((position_x IS NULL AND position_y IS NULL AND position_z IS NULL)
        OR (position_x IS NOT NULL AND position_y IS NOT NULL AND position_z IS NOT NULL)),
    UNIQUE (item_id, location_id)
);
CREATE INDEX stock_lots_item_idx ON stock_lots(item_id);
CREATE INDEX stock_lots_location_idx ON stock_lots(location_id);
CREATE TABLE reservations (
    id uuid PRIMARY KEY,
    location_id uuid NOT NULL REFERENCES inventory_locations(id) ON DELETE RESTRICT,
    label text NOT NULL CHECK (btrim(label) <> ''),
    notes text
);
CREATE INDEX reservations_location_idx ON reservations(location_id);
CREATE TABLE occupancies (
    location_id uuid PRIMARY KEY REFERENCES inventory_locations(id) ON DELETE CASCADE,
    state text NOT NULL CHECK (state IN ('free', 'occupied', 'reserved', 'blocked')),
    fill_percent smallint CHECK (fill_percent BETWEEN 0 AND 100)
);
