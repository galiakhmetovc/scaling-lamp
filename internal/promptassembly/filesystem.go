package promptassembly

type FilesystemHeadInput struct {
	Recent FilesystemRecentHeadInput
	Tree   []FilesystemTreeEntry
}

type FilesystemRecentHeadInput struct {
	Edited  []string
	Read    []string
	Found   []string
	Moved   []string
	Trashed []string
}

type FilesystemTreeEntry struct {
	Name  string
	IsDir bool
}
