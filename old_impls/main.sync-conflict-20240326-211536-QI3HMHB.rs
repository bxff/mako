use std::cmp::Ord;
use std::fmt;

const NODE_SIZE: usize = 3;

#[derive(Debug)]
struct Node<T> {
    elements: Vec<T>,
    children: Vec<Node<T>>,
}

impl<T> Node<T> {
    fn new() -> Self {
        Node {
            elements: Vec::new(),
            children: Vec::new(),
        }
    }

    fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

pub struct BTree<T>
where
    T: Ord,
{
    root: Option<Node<T>>,
}

impl<T> BTree<T>
where
    T: Ord + fmt::Debug,
{
   fn insert(&mut self, element:T, index:i32){
       //Implementation here 
   }

   fn lookup_index(&self, i:i32) -> Option<(T,i32)>{
       //Implementation here 
       None
   }
}


#[cfg(test)]
mod tests {

use super::*;

fn create_sample_btree() -> BTree<char>{
     let mut btree = BTree{root=None};
     //inserting elements A-Z at indexes 0-25 in order into b-tree.
     for i in 0..26 as i32{
         btree.insert((i+65).to_string(),i);
     }  
     
     return btree;
} 

 #[test]
fn test_lookup(){
      let btree = create_sample_btree();
      
      assert_eq!(btree.lookup_index(12), Some(("M".to_string(),12)));
      assert_eq!(btree.lookup_index(4), Some(("E".to_string(),4)));
      assert_eq!(btree.lookup_index(25), Some(("Z".to_string(),25)));

}


}