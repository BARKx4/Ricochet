create table journal_entries (
  id integer primary key,
  title text not null,
  body text not null,
  status text not null default 'draft',
  mood text not null default 'steady',
  created_on text not null
);
