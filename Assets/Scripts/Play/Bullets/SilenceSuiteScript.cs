using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SilenceSuiteScript : MonoBehaviour
{
    public float speed = 7;
    public GameObject SC;
    GameObject cA;
    GameObject cB;
    public LineRenderer LRR;
    public Fix64 turnangle = Fix64.Pi / (Fix64)12;
    public float maxtime = 1;
    float timepsd = 0;
    //public Rigidbody2D rA;
    //public Rigidbody2D rB;

	// Use this for initialization
	void Start ()
    {
        //rA = cA.GetComponent<Rigidbody2D>();
        //rB = cB.GetComponent<Rigidbody2D>();
    }
	
	// Update is called once per frame
	void Update () {

    }

    void FixedUpdate()
    {
        timepsd += Time.fixedDeltaTime;
        if (timepsd >= maxtime)
        {
            endwork();
            enabled = false;
        }
    }

    public void work(Fix64Vector2 workdir)
    {
        Fix64Vector2 s = workdir.normalized() * (Fix64)speed;
        Fix64Vector2 sA = s.CCWTurn(turnangle);
        Fix64Vector2 sB = s.CCWTurn(-turnangle);
        maxtime = (float)(workdir.Length() * (Fix64)2 / (sA + sB).Length());
        cA = Instantiate(SC, transform.position, Quaternion.identity);
        cB = Instantiate(SC, transform.position, Quaternion.identity);
        cA.GetComponent<Rigidbody2D>().velocity = sA.ToV2();
        cB.GetComponent<Rigidbody2D>().velocity = sB.ToV2();
        //enabled = true;
    }
    
    void endwork()
    {
        cA.GetComponent<Rigidbody2D>().velocity = Vector2.zero;
        cB.GetComponent<Rigidbody2D>().velocity = Vector2.zero;
        Vector2 RayV2 = cB.GetComponent<Rigidbody2D>().position - cA.GetComponent<Rigidbody2D>().position;
        RaycastHit2D[] Allhit = Physics2D.RaycastAll(cA.GetComponent<Rigidbody2D>().position, RayV2);
        foreach (RaycastHit2D hit in Allhit)
        {
            if (hit.collider.GetComponent<DoSkill>() == null)
                continue;
            hit.collider.GetComponent<DoSkill>().GetTied(3);
        }
        cA.GetComponent<DestroyScript>().Destroyself();
        cB.GetComponent<DestroyScript>().Destroyself();
        SSLine(cA.GetComponent<Rigidbody2D>().position, cB.GetComponent<Rigidbody2D>().position);
    }

    void SSLine(Vector2 a, Vector2 b)
    {
        LRR.SetPosition(0, a);
        LRR.SetPosition(1, b);
        StartCoroutine(byebye());
    }

    IEnumerator byebye()
    {
        yield return new WaitForSeconds(0.2f);
        GetComponent<DestroyScript>().Destroyself();
    }
}
