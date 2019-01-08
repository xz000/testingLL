using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class RollScript : MonoBehaviour
{
    public Vector2 v;
    public Rigidbody2D selfRB;
    public Fix64 rolldamage = (Fix64)2;

    private void FixedUpdate()
    {
         //if (!photonView.isMine)
            return;
        selfRB.velocity = v;
        rollSkill();
    }
    
    public void rollSkill()
    {
        float radius = transform.lossyScale.x / 2;
        Vector2 actionplace = transform.position;
        Collider2D[] colliders = Physics2D.OverlapCircleAll(actionplace, radius);
        foreach (Collider2D hit in colliders)
        {
            HPScript hp = hit.GetComponent<HPScript>();
            if (hp != null)
            {
                hp.GetHurt(rolldamage * (Fix64)Time.fixedDeltaTime);
            }
        }
    }
}
