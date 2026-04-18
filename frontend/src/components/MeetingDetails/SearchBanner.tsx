'use client';

import React from 'react';
import { ArrowLeft, ChevronLeft, ChevronRight, X, Search } from 'lucide-react';
import { useRouter } from 'next/navigation';

interface SearchBannerProps {
  searchTerm: string;
  currentIndex: number;
  totalMatches: number;
  onPrev: () => void;
  onNext: () => void;
  onClose: () => void;
}

export function SearchBanner({
  searchTerm,
  currentIndex,
  totalMatches,
  onPrev,
  onNext,
  onClose,
}: SearchBannerProps) {
  const router = useRouter();

  return (
    <div className="sticky top-0 z-20 bg-yellow-50 border-b border-yellow-200 px-4 py-2 flex items-center gap-3">
      <button
        onClick={() => router.push('/meetings')}
        className="flex items-center gap-1 text-sm text-gray-600 hover:text-gray-800 transition-colors"
      >
        <ArrowLeft className="w-4 h-4" />
        <span>Retour</span>
      </button>

      <div className="flex items-center gap-2 ml-4">
        <Search className="w-4 h-4 text-yellow-600" />
        <span className="text-sm font-medium text-yellow-800">&quot;{searchTerm}&quot;</span>
      </div>

      <div className="flex items-center gap-1 ml-4">
        <button
          onClick={onPrev}
          disabled={currentIndex <= 0}
          className="p-1 rounded hover:bg-yellow-100 disabled:opacity-30 disabled:cursor-not-allowed"
        >
          <ChevronLeft className="w-4 h-4" />
        </button>
        <span className="text-sm text-gray-600 min-w-[3rem] text-center">
          {totalMatches > 0 ? `${currentIndex + 1}/${totalMatches}` : '0/0'}
        </span>
        <button
          onClick={onNext}
          disabled={currentIndex >= totalMatches - 1}
          className="p-1 rounded hover:bg-yellow-100 disabled:opacity-30 disabled:cursor-not-allowed"
        >
          <ChevronRight className="w-4 h-4" />
        </button>
      </div>

      <button
        onClick={onClose}
        className="ml-auto p-1 rounded hover:bg-yellow-100"
      >
        <X className="w-4 h-4 text-gray-500" />
      </button>
    </div>
  );
}
