create table contacts (
  id integer primary key,
  name text not null,
  email text not null unique,
  status text not null default 'active'
);
