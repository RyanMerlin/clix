package clix

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
)

func loadManifestsFromDir[T any](dir string, fn func(string) (T, error)) ([]T, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var out []T
	for _, entry := range entries {
		if entry.IsDir() || filepath.Ext(entry.Name()) != ".json" {
			continue
		}
		item, err := fn(filepath.Join(dir, entry.Name()))
		if err == nil {
			out = append(out, item)
		}
	}
	return out, nil
}

func loadPackManifests(dir string) ([]PackManifest, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var out []PackManifest
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		packPath := filepath.Join(dir, entry.Name(), "pack.json")
		var manifest PackManifest
		if err := readJSON(packPath, &manifest); err == nil {
			out = append(out, manifest)
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out, nil
}

func seedBuiltinPack(base State, pack PackManifest) error {
	path := filepath.Join(base.PacksDir, pack.Name)
	if fileExists(path) {
		return nil
	}
	if err := ensureDir(path); err != nil {
		return err
	}
	return writeJSON(filepath.Join(path, "pack.json"), pack)
}

func seedBuiltinPacks(base State) error {
	packs := []PackManifest{
		{Name: "base", Version: 1, Description: "Shared safe defaults.", Profiles: []string{"base"}},
		{Name: "gcloud-readonly-planning", Version: 1, Description: "Read-only gcloud planning.", Profiles: []string{"gcloud-readonly-planning"}},
		{Name: "gcloud-vertex-ai-operator", Version: 1, Description: "Vertex AI operations.", Profiles: []string{"gcloud-vertex-ai-operator"}},
		{Name: "kubectl-observe", Version: 1, Description: "Read-only kubectl inspection.", Profiles: []string{"kubectl-observe"}},
		{Name: "kubectl-change-controlled", Version: 1, Description: "Guarded kubectl change operations.", Profiles: []string{"kubectl-change-controlled"}},
		{Name: "gh-readonly", Version: 1, Description: "Read-only GitHub CLI inspection.", Profiles: []string{"gh-readonly"}},
		{Name: "git-observer", Version: 1, Description: "Git workspace inspection.", Profiles: []string{"git-observer"}},
		{Name: "infisical-readonly", Version: 1, Description: "Read-only Infisical inspection.", Profiles: []string{"infisical-readonly"}},
		{Name: "incus-readonly", Version: 1, Description: "Read-only Incus inspection.", Profiles: []string{"incus-readonly"}},
		{Name: "argocd-observe", Version: 1, Description: "Read-only Argo CD inspection.", Profiles: []string{"argocd-observe"}},
	}
	for _, pack := range packs {
		if err := seedBuiltinPack(base, pack); err != nil {
			return err
		}
	}
	return nil
}

func copyDir(src, dst string) error {
	entries, err := os.ReadDir(src)
	if err != nil {
		return err
	}
	if err := ensureDir(dst); err != nil {
		return err
	}
	for _, entry := range entries {
		sourcePath := filepath.Join(src, entry.Name())
		targetPath := filepath.Join(dst, entry.Name())
		if entry.IsDir() {
			if err := copyDir(sourcePath, targetPath); err != nil {
				return err
			}
			continue
		}
		in, err := os.Open(sourcePath)
		if err != nil {
			return err
		}
		out, err := os.Create(targetPath)
		if err != nil {
			in.Close()
			return err
		}
		if _, err := io.Copy(out, in); err != nil {
			in.Close()
			out.Close()
			return err
		}
		if err := in.Close(); err != nil {
			out.Close()
			return err
		}
		if err := out.Close(); err != nil {
			return err
		}
	}
	return nil
}

func installPack(sourceDir, packsDir string, force bool) (PackManifest, error) {
	var manifest PackManifest
	if err := readJSON(filepath.Join(sourceDir, "pack.json"), &manifest); err != nil {
		return PackManifest{}, fmt.Errorf("read pack manifest: %w", err)
	}
	dest := filepath.Join(packsDir, manifest.Name)
	if fileExists(dest) {
		if !force {
			return PackManifest{}, fmt.Errorf("pack already installed: %s", manifest.Name)
		}
		if err := os.RemoveAll(dest); err != nil {
			return PackManifest{}, err
		}
	}
	if err := copyDir(sourceDir, dest); err != nil {
		return PackManifest{}, err
	}
	return manifest, nil
}

func discoverPack(path string) (PackManifest, error) {
	var manifest PackManifest
	if err := readJSON(filepath.Join(path, "pack.json"), &manifest); err != nil {
		return PackManifest{}, err
	}
	return manifest, nil
}
