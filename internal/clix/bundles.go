package clix

import (
	"archive/zip"
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

func bundlePack(sourceDir, outPath string) (PackBundleManifest, string, error) {
	var pack PackManifest
	if err := readJSON(filepath.Join(sourceDir, "pack.json"), &pack); err != nil {
		return PackBundleManifest{}, "", fmt.Errorf("read pack manifest: %w", err)
	}
	files, err := collectBundleFiles(sourceDir)
	if err != nil {
		return PackBundleManifest{}, "", err
	}
	bundle := PackBundleManifest{
		SchemaVersion: 1,
		CreatedAt:     currentISO(),
		Pack:          pack,
		Files:         files,
	}
	if outPath == "" {
		outPath = defaultBundlePath(pack)
	}
	if err := ensureDir(filepath.Dir(outPath)); err != nil {
		return PackBundleManifest{}, "", err
	}
	if err := writeBundleZip(outPath, sourceDir, bundle); err != nil {
		return PackBundleManifest{}, "", err
	}
	sum, err := hashFile(outPath)
	if err != nil {
		return PackBundleManifest{}, "", err
	}
	if err := os.WriteFile(outPath+".sha256", []byte(sum+"\n"), 0o644); err != nil {
		return PackBundleManifest{}, "", err
	}
	return bundle, outPath, nil
}

func publishPack(sourcePath, toDir string, force bool) (map[string]any, error) {
	bundlePath, bundle, err := prepareBundleArtifact(sourcePath)
	if err != nil {
		return nil, err
	}
	if !isArchivePath(sourcePath) {
		defer os.Remove(bundlePath)
		defer os.Remove(bundlePath + ".sha256")
	}
	if toDir == "" {
		toDir = filepath.Join(HomeDir(), "bundles", "published")
	}
	if err := ensureDir(toDir); err != nil {
		return nil, err
	}
	targetBundle := filepath.Join(toDir, filepath.Base(bundlePath))
	if fileExists(targetBundle) && !force {
		return nil, fmt.Errorf("bundle already exists: %s", targetBundle)
	}
	if err := copyFile(bundlePath, targetBundle); err != nil {
		return nil, err
	}
	if fileExists(bundlePath + ".sha256") {
		if err := copyFile(bundlePath+".sha256", targetBundle+".sha256"); err != nil {
			return nil, err
		}
	}
	indexPath := filepath.Join(toDir, "index.json")
	index := map[string]any{
		"schemaVersion": 1,
		"publishedAt":   currentISO(),
		"bundle":        bundle,
		"archive":       filepath.Base(targetBundle),
	}
	_ = writeJSON(indexPath, index)
	return map[string]any{
		"ok":      true,
		"bundle":  bundle,
		"archive": targetBundle,
		"sha256":  targetBundle + ".sha256",
		"index":   indexPath,
	}, nil
}

func prepareBundleArtifact(sourcePath string) (string, PackBundleManifest, error) {
	if fileExists(sourcePath) && !isArchivePath(sourcePath) {
		var bundle PackBundleManifest
		if err := readJSON(filepath.Join(sourcePath, "pack.json"), &bundle.Pack); err != nil {
			return "", PackBundleManifest{}, err
		}
		files, err := collectBundleFiles(sourcePath)
		if err != nil {
			return "", PackBundleManifest{}, err
		}
		bundle = PackBundleManifest{SchemaVersion: 1, CreatedAt: currentISO(), Pack: bundle.Pack, Files: files}
		tmp, err := os.CreateTemp("", "clix-bundle-*.zip")
		if err != nil {
			return "", PackBundleManifest{}, err
		}
		tmpPath := tmp.Name()
		tmp.Close()
		if err := writeBundleZip(tmpPath, sourcePath, bundle); err != nil {
			_ = os.Remove(tmpPath)
			return "", PackBundleManifest{}, err
		}
		sum, err := hashFile(tmpPath)
		if err != nil {
			_ = os.Remove(tmpPath)
			return "", PackBundleManifest{}, err
		}
		if err := os.WriteFile(tmpPath+".sha256", []byte(sum+"\n"), 0o644); err != nil {
			_ = os.Remove(tmpPath)
			return "", PackBundleManifest{}, err
		}
		return tmpPath, bundle, nil
	}
	if isArchivePath(sourcePath) {
		if err := verifyBundleChecksum(sourcePath); err != nil {
			return "", PackBundleManifest{}, err
		}
		bundle, err := readBundleManifest(sourcePath)
		if err != nil {
			return "", PackBundleManifest{}, err
		}
		return sourcePath, bundle, nil
	}
	return "", PackBundleManifest{}, fmt.Errorf("unknown bundle source: %s", sourcePath)
}

func installPack(sourceDir, packsDir string, force bool) (PackManifest, error) {
	if isArchivePath(sourceDir) {
		tempDir, err := os.MkdirTemp("", "clix-install-*")
		if err != nil {
			return PackManifest{}, err
		}
		defer os.RemoveAll(tempDir)
		if err := extractBundleArchive(sourceDir, tempDir); err != nil {
			return PackManifest{}, err
		}
		return installPack(tempDir, packsDir, force)
	}
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
	if isArchivePath(path) {
		tempDir, err := os.MkdirTemp("", "clix-discover-*")
		if err != nil {
			return PackManifest{}, err
		}
		defer os.RemoveAll(tempDir)
		if err := extractBundleArchive(path, tempDir); err != nil {
			return PackManifest{}, err
		}
		path = tempDir
	}
	var manifest PackManifest
	if err := readJSON(filepath.Join(path, "pack.json"), &manifest); err != nil {
		return PackManifest{}, err
	}
	return manifest, nil
}

func defaultBundlePath(pack PackManifest) string {
	name := pack.Name
	if name == "" {
		name = "pack"
	}
	return filepath.Join(".", fmt.Sprintf("%s-v%d.clixpack.zip", name, pack.Version))
}

func isArchivePath(path string) bool {
	ext := strings.ToLower(filepath.Ext(path))
	return ext == ".zip" || ext == ".clixpack"
}

func collectBundleFiles(sourceDir string) ([]PackBundleFile, error) {
	var files []PackBundleFile
	err := filepath.WalkDir(sourceDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			return nil
		}
		rel, err := filepath.Rel(sourceDir, path)
		if err != nil {
			return err
		}
		sum, err := hashFile(path)
		if err != nil {
			return err
		}
		info, err := d.Info()
		if err != nil {
			return err
		}
		files = append(files, PackBundleFile{
			Path:   filepath.ToSlash(rel),
			Size:   info.Size(),
			SHA256: sum,
		})
		return nil
	})
	if err != nil {
		return nil, err
	}
	sort.Slice(files, func(i, j int) bool { return files[i].Path < files[j].Path })
	return files, nil
}

func writeBundleZip(outPath, sourceDir string, bundle PackBundleManifest) error {
	f, err := os.Create(outPath)
	if err != nil {
		return err
	}
	defer f.Close()

	zw := zip.NewWriter(f)
	if err := writeZipJSON(zw, "bundle.json", bundle); err != nil {
		_ = zw.Close()
		return err
	}
	err = filepath.WalkDir(sourceDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			return nil
		}
		rel, err := filepath.Rel(sourceDir, path)
		if err != nil {
			return err
		}
		name := filepath.ToSlash(rel)
		if name == "bundle.json" {
			return nil
		}
		return writeZipFile(zw, path, name)
	})
	if err != nil {
		_ = zw.Close()
		return err
	}
	if err := zw.Close(); err != nil {
		return err
	}
	return nil
}

func writeZipJSON(zw *zip.Writer, name string, v any) error {
	b, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return err
	}
	hdr := &zip.FileHeader{Name: name, Method: zip.Deflate}
	hdr.SetModTime(time.Now().UTC())
	w, err := zw.CreateHeader(hdr)
	if err != nil {
		return err
	}
	_, err = w.Write(append(b, '\n'))
	return err
}

func writeZipFile(zw *zip.Writer, sourcePath, name string) error {
	b, err := os.ReadFile(sourcePath)
	if err != nil {
		return err
	}
	hdr := &zip.FileHeader{Name: name, Method: zip.Deflate}
	hdr.SetModTime(time.Now().UTC())
	w, err := zw.CreateHeader(hdr)
	if err != nil {
		return err
	}
	_, err = io.Copy(w, bytes.NewReader(b))
	return err
}

func readBundleManifest(path string) (PackBundleManifest, error) {
	r, err := zip.OpenReader(path)
	if err != nil {
		return PackBundleManifest{}, err
	}
	defer r.Close()
	for _, file := range r.File {
		if file.Name == "bundle.json" {
			rc, err := file.Open()
			if err != nil {
				return PackBundleManifest{}, err
			}
			defer rc.Close()
			var bundle PackBundleManifest
			if err := json.NewDecoder(rc).Decode(&bundle); err != nil {
				return PackBundleManifest{}, err
			}
			return bundle, nil
		}
	}
	return PackBundleManifest{}, fmt.Errorf("bundle manifest not found: %s", path)
}

func verifyBundleChecksum(path string) error {
	expectedPath := path + ".sha256"
	if !fileExists(expectedPath) {
		return nil
	}
	expected, err := os.ReadFile(expectedPath)
	if err != nil {
		return err
	}
	actual, err := hashFile(path)
	if err != nil {
		return err
	}
	if strings.TrimSpace(string(expected)) != actual {
		return fmt.Errorf("bundle checksum mismatch: %s", path)
	}
	return nil
}

func extractBundleArchive(path, targetDir string) error {
	r, err := zip.OpenReader(path)
	if err != nil {
		return err
	}
	defer r.Close()
	for _, file := range r.File {
		targetPath, err := safeJoin(targetDir, file.Name)
		if err != nil {
			return err
		}
		if file.FileInfo().IsDir() {
			if err := ensureDir(targetPath); err != nil {
				return err
			}
			continue
		}
		if err := ensureDir(filepath.Dir(targetPath)); err != nil {
			return err
		}
		rc, err := file.Open()
		if err != nil {
			return err
		}
		out, err := os.Create(targetPath)
		if err != nil {
			rc.Close()
			return err
		}
		if _, err := io.Copy(out, rc); err != nil {
			rc.Close()
			out.Close()
			return err
		}
		if err := rc.Close(); err != nil {
			out.Close()
			return err
		}
		if err := out.Close(); err != nil {
			return err
		}
	}
	return nil
}

func safeJoin(root, name string) (string, error) {
	cleaned := filepath.Clean(filepath.FromSlash(name))
	if cleaned == "." || cleaned == string(os.PathSeparator) {
		return "", fmt.Errorf("invalid archive path: %s", name)
	}
	target := filepath.Join(root, cleaned)
	rel, err := filepath.Rel(root, target)
	if err != nil {
		return "", err
	}
	if rel == ".." || strings.HasPrefix(rel, ".."+string(os.PathSeparator)) {
		return "", fmt.Errorf("invalid archive path: %s", name)
	}
	return target, nil
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	if err := ensureDir(filepath.Dir(dst)); err != nil {
		return err
	}
	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer out.Close()
	_, err = io.Copy(out, in)
	return err
}

func hashFile(path string) (string, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(b)
	return hex.EncodeToString(sum[:]), nil
}
