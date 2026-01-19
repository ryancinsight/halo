use halo::{BrandedVec, GhostToken, BrandedHashMap};

fn main() {
    GhostToken::new(|mut token| {
        // Case 1: BrandedVec
        let mut vec = BrandedVec::new();
        vec.push(1);
        vec.push(2);

        {
            // We get a mutable slice using &self and &mut token
            let slice = vec.as_mut_slice(&mut token);

            // Can we still read vec?
            println!("Vec len: {}", vec.len()); // Should be allowed because vec is borrowed immutably

            slice[0] = 10;
        }

        // Case 2: BrandedHashMap
        let mut map = BrandedHashMap::new();
        map.insert(1, 10);
        map.insert(2, 20);

        // We want to iterate mutably while reading map
        // currently BrandedHashMap does not have iter_mut.
        // It has for_each_mut.

        map.for_each_mut(&mut token, |k, v| {
             // Can we read map here?
             // for_each_mut takes &self.
             println!("Map len inside for_each: {}", map.len()); // Should be allowed
             *v += 1;
        });

        // But we want an Iterator.
        // let iter = map.iter_mut(&mut token);
        // println!("Map len during iter: {}", map.len());
        // iter.for_each(|(k, v)| *v += 1);
    });
}
