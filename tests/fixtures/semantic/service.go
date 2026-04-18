package handler

import (
	"context"
	"errors"
)

// DefaultUserService is the concrete implementation of UserService.
type DefaultUserService struct {
	repo UserRepo
}

// UserRepo is the persistence interface.
type UserRepo interface {
	Insert(ctx context.Context, name string) (int64, error)
	FindByName(ctx context.Context, name string) (*User, error)
}

// NewDefaultUserService creates a DefaultUserService.
func NewDefaultUserService(repo UserRepo) *DefaultUserService {
	return &DefaultUserService{repo: repo}
}

// Save persists a new user.
func (s *DefaultUserService) Save(ctx context.Context, name string) error {
	if name == "" {
		return errors.New("name required")
	}
	_, err := s.repo.Insert(ctx, name)
	return err
}

// FindByName retrieves a user by name.
func (s *DefaultUserService) FindByName(ctx context.Context, name string) (*User, error) {
	return s.repo.FindByName(ctx, name)
}
