using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public delegate void CookVelo(Rigidbody2D victim,MoveScript worker);

public class MoveScript : MonoBehaviour {

    public bool controllable;
    //public bool cooking;
    public float movespeed;
    public Vector2 movedirection = Vector2.zero;
    public GameObject targetshadow;
    public Vector2 Givenvelocity;
    public Vector2 VelotoAdd = Vector2.zero;
    private Vector2 movetarget;
    public Vector2 selfvelocity;
    public Rigidbody2D PlayerRb2d;
    GameObject targeticon;
    private Vector3 followplace;
    public CookVelo cook;
    public bool fixposlater = false;

    void cookstart(Rigidbody2D victim, MoveScript worker)
    {
        worker.VelotoAdd = Vector2.zero;
    }

	// Use this for initialization
	void Start ()
    {
        PlayerRb2d = GetComponent<Rigidbody2D>();
        controllable = true;
        cook = cookstart;
    }

    void Setselfvelocity()
    {
        if (targeticon != null)
        {
            movetarget = targeticon.transform.position;
            movedirection = movetarget - PlayerRb2d.position;
            if (movedirection.sqrMagnitude < 0.1)
            {
                stopwalking();
            }
            selfvelocity = movedirection.normalized * movespeed;// + StealthScript.Speed);
        }
        else
        {
            selfvelocity = Vector2.zero;
        }
    }

    public void SetTarget(Vector2 rcplace)
    {
        GameObject.Destroy(targeticon);
        targeticon = Instantiate(targetshadow, rcplace, Quaternion.identity);
        //DoSkill.singing = 0;
        //GetComponent<SkillR2b>().IdoDSWL();
    }

    void FixedUpdate()
    {
        Setselfvelocity();
        SetTotalVelocity();
        //FixSelfPos();
    }

    void SetTotalVelocity()
    {
        if (controllable)
        {
            cook(PlayerRb2d, this);
            PlayerRb2d.velocity = selfvelocity + VelotoAdd;
        }
        else
        {
            PlayerRb2d.velocity = Givenvelocity;
        }
    }

    /*void FixSelfPos()
    {
        if (PlayerRb2d.position != Vector2.zero)
        {
            fixposlater = true;
            Debug.Log("ash");
            return;
        }
        if (fixposlater)
        {
            PlayerRb2d.position = Fix256.F256v2(PlayerRb2d.position);
            fixposlater = false;
            Debug.Log("Smash");
        }
    }*/

    public void stopwalking()
    {
        GameObject.Destroy(targeticon);
        //GetComponent<SkillR2b>().IdoDSWL();
    }
}
